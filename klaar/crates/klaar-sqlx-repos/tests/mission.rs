//! Story 3.4 — attribution atomique, contre un vrai PostgreSQL (FR-013).
//!
//! **C'est le seul endroit où la course se teste.** Un double en mémoire dirait
//! ce qu'on lui a fait dire ; la garantie vient de PostgreSQL, qui sérialise
//! les écritures sur une même ligne. Ces cas lancent donc de vraies
//! acceptations concurrentes, sur des connexions distinctes, et vérifient
//! qu'une seule aboutit.

use chrono::{Duration, Utc};
use klaar_application::ports::demande_repository::DemandeRepository;
use klaar_application::ports::mission_repository::{MissionRepository, ResultatAttribution};
use klaar_application::ports::provider_repository::ProviderRepository;
use klaar_catalog::CodeCatalogue;
use klaar_identity::{
    EmpreinteMotDePasse, MotDePasse, NumeroBce, ParametresArgon2, PreuveKyc, Provider,
};
use klaar_matching::{Demande, Urgence};
use klaar_shared_kernel::{Email, Geo};
use klaar_sqlx_repos::demonstration::compte_actif_de_demonstration;
use klaar_sqlx_repos::{
    creer_pool, PgDemandeRepository, PgMissionRepository, PgProviderRepository, PoolPg,
};
use std::sync::Arc;
use uuid::Uuid;

/// Grand-Place.
const CENTRE: (f64, f64) = (50.8467, 4.3525);

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

/// Numéro BCE construit et tiré au sort, jamais copié d'une entreprise réelle.
fn numero() -> NumeroBce {
    let corps = (Uuid::new_v4().as_u128() as u64) % 20_000_000;
    NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).expect("numéro construit")
}

async fn compte(pool: &PoolPg, marqueur: &str) -> Uuid {
    let email = Email::parse(&format!("mission-{marqueur}-{}@example.eu", Uuid::new_v4())).unwrap();
    let empreinte = EmpreinteMotDePasse::calculer(
        &MotDePasse::parse("Marie@2026Secure").unwrap(),
        ParametresArgon2::tests(),
    )
    .unwrap();
    compte_actif_de_demonstration(pool, &email, &empreinte)
        .await
        .expect("compte de test")
}

async fn prestataire(pool: &PoolPg, marqueur: &str) -> Provider {
    let depot = PgProviderRepository::new(pool.clone());
    let mut p = Provider::inscrire(
        compte(pool, marqueur).await,
        numero(),
        &format!("Prestataire {marqueur}"),
        Geo::new(CENTRE.0, CENTRE.1).unwrap(),
        vec![CodeCatalogue::parse("plomberie").unwrap()],
        Utc::now(),
    )
    .expect("prestataire valide");
    p.valider_kyc(PreuveKyc::demonstration(Utc::now()));
    depot.creer(&p).await.expect("création");
    p
}

async fn demande(pool: &PoolPg) -> Demande {
    let depot = PgDemandeRepository::new(pool.clone());
    let d = Demande::soumettre(
        compte(pool, "demandeur").await,
        CodeCatalogue::parse("plomberie").unwrap(),
        "Fuite sous l'évier",
        Geo::new(CENTRE.0, CENTRE.1).unwrap(),
        Urgence::Haute,
        Utc::now(),
    )
    .expect("Demande valide");
    depot.creer(&d).await.expect("création");
    d
}

#[tokio::test]
async fn happy_une_acceptation_attribue_la_demande_et_cree_la_mission() {
    let pool = pool().await;
    let missions = PgMissionRepository::new(pool.clone());
    let demandes = PgDemandeRepository::new(pool.clone());
    let d = demande(&pool).await;
    let p = prestataire(&pool, "gagnant").await;

    let maintenant = Utc::now();
    let issue = missions.attribuer(d.id, p.id, maintenant).await.unwrap();
    let ResultatAttribution::Attribuee(mission) = issue else {
        panic!("attendu une attribution, obtenu {issue:?}");
    };
    assert_eq!(mission.demande_id, d.id);
    assert_eq!(mission.provider_id, p.id);
    assert_eq!(mission.statut.as_str(), "ACCEPTED");

    // La Demande **et** la Mission, dans la même transaction : une Demande
    // attribuée sans Mission promettrait une intervention dont personne ne
    // porte la trace.
    let relue = demandes.par_id(d.id).await.unwrap().expect("la Demande");
    assert_eq!(relue.statut.as_str(), "MATCHED");
    assert_eq!(
        missions.en_cours_pour(p.id).await.unwrap().map(|m| m.id),
        Some(mission.id)
    );
}

#[tokio::test]
async fn edge_deux_acceptations_simultanees_n_en_laissent_passer_qu_une() {
    // Le cas que toute cette story existe pour fermer. Deux tâches, deux
    // connexions, la même Demande : sans le compare-and-swap, deux
    // camionnettes partent pour une seule fuite.
    let pool = pool().await;
    let d = demande(&pool).await;
    let a = prestataire(&pool, "course-a").await;
    let b = prestataire(&pool, "course-b").await;

    let depot = Arc::new(PgMissionRepository::new(pool.clone()));
    let maintenant = Utc::now();
    let (da, db) = (depot.clone(), depot.clone());
    let (ida, idb) = (d.id, d.id);
    let (pa, pb) = (a.id, b.id);

    let (ra, rb) = tokio::join!(
        tokio::spawn(async move { da.attribuer(ida, pa, maintenant).await.unwrap() }),
        tokio::spawn(async move { db.attribuer(idb, pb, maintenant).await.unwrap() }),
    );
    let issues = [ra.unwrap(), rb.unwrap()];

    let gagnants = issues
        .iter()
        .filter(|i| matches!(i, ResultatAttribution::Attribuee(_)))
        .count();
    let perdants = issues
        .iter()
        .filter(|i| matches!(i, ResultatAttribution::DemandeNonDiffusee))
        .count();
    assert_eq!(gagnants, 1, "issues : {issues:?}");
    assert_eq!(perdants, 1, "issues : {issues:?}");

    // Et une seule Mission en base, pas deux.
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mission WHERE demande_id = $1")
        .bind(d.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 1);
}

#[tokio::test]
async fn negative_une_demande_deja_attribuee_ne_se_reprend_pas() {
    let pool = pool().await;
    let missions = PgMissionRepository::new(pool.clone());
    let d = demande(&pool).await;
    let premier = prestataire(&pool, "premier").await;
    let tardif = prestataire(&pool, "tardif").await;

    missions
        .attribuer(d.id, premier.id, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        missions
            .attribuer(d.id, tardif.id, Utc::now())
            .await
            .unwrap(),
        ResultatAttribution::DemandeNonDiffusee
    );
}

#[tokio::test]
async fn negative_un_prestataire_deja_en_mission_ne_peut_pas_en_prendre_une_seconde() {
    // Politique MVP : une Mission à la fois (FR-013 `@edge`).
    let pool = pool().await;
    let missions = PgMissionRepository::new(pool.clone());
    let p = prestataire(&pool, "occupe").await;
    let premiere = demande(&pool).await;
    let seconde = demande(&pool).await;

    missions
        .attribuer(premiere.id, p.id, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        missions
            .attribuer(seconde.id, p.id, Utc::now())
            .await
            .unwrap(),
        ResultatAttribution::ProviderOccupe
    );
}

#[tokio::test]
async fn edge_une_acceptation_refusee_pour_occupation_laisse_la_demande_diffusee() {
    // Sans le `ROLLBACK`, un prestataire déjà occupé éteindrait la Demande en
    // essayant de l'accepter : elle resterait `MATCHED` sans Mission, et
    // personne d'autre ne pourrait la prendre.
    let pool = pool().await;
    let missions = PgMissionRepository::new(pool.clone());
    let demandes = PgDemandeRepository::new(pool.clone());
    let occupe = prestataire(&pool, "deja-pris").await;
    let libre = prestataire(&pool, "libre").await;
    let premiere = demande(&pool).await;
    let convoitee = demande(&pool).await;

    missions
        .attribuer(premiere.id, occupe.id, Utc::now())
        .await
        .unwrap();
    missions
        .attribuer(convoitee.id, occupe.id, Utc::now())
        .await
        .unwrap();

    let relue = demandes
        .par_id(convoitee.id)
        .await
        .unwrap()
        .expect("la Demande");
    assert_eq!(relue.statut.as_str(), "BROADCASTING");
    assert!(matches!(
        missions
            .attribuer(convoitee.id, libre.id, Utc::now())
            .await
            .unwrap(),
        ResultatAttribution::Attribuee(_)
    ));
}

#[tokio::test]
async fn edge_dix_acceptations_simultanees_n_en_laissent_passer_qu_une() {
    // Cinq prestataires notifiés, dix tentatives : la borne ne dépend pas du
    // nombre de concurrents.
    let pool = pool().await;
    let d = demande(&pool).await;
    let depot = Arc::new(PgMissionRepository::new(pool.clone()));
    let maintenant = Utc::now();

    let mut taches = Vec::new();
    for i in 0..10 {
        let p = prestataire(&pool, &format!("foule-{i}")).await;
        let depot = depot.clone();
        let id = d.id;
        taches.push(tokio::spawn(async move {
            depot.attribuer(id, p.id, maintenant).await.unwrap()
        }));
    }
    let mut gagnants = 0;
    for t in taches {
        if matches!(t.await.unwrap(), ResultatAttribution::Attribuee(_)) {
            gagnants += 1;
        }
    }
    assert_eq!(gagnants, 1);
}

#[tokio::test]
async fn security_une_demande_annulee_ne_peut_pas_etre_attribuee() {
    // La garde du compare-and-swap porte sur `BROADCASTING`, pas sur « pas
    // encore attribuée » : accepter une Demande annulée enverrait quelqu'un
    // chez un demandeur qui a explicitement dit non.
    let pool = pool().await;
    let missions = PgMissionRepository::new(pool.clone());
    let demandes = PgDemandeRepository::new(pool.clone());
    let d = demande(&pool).await;
    let p = prestataire(&pool, "sur-annulee").await;

    demandes
        .changer_statut(d.id, klaar_matching::StatutDemande::Annulee, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        missions.attribuer(d.id, p.id, Utc::now()).await.unwrap(),
        ResultatAttribution::DemandeNonDiffusee
    );
}

#[tokio::test]
async fn security_une_demande_sans_reponse_ne_peut_pas_etre_attribuee() {
    let pool = pool().await;
    let missions = PgMissionRepository::new(pool.clone());
    let demandes = PgDemandeRepository::new(pool.clone());
    let d = demande(&pool).await;
    let p = prestataire(&pool, "sur-nomatch").await;

    demandes
        .changer_statut(d.id, klaar_matching::StatutDemande::SansReponse, Utc::now())
        .await
        .unwrap();
    assert_eq!(
        missions.attribuer(d.id, p.id, Utc::now()).await.unwrap(),
        ResultatAttribution::DemandeNonDiffusee
    );
}

#[tokio::test]
async fn edge_la_mission_porte_l_instant_qu_on_lui_donne() {
    // L'horloge est injectée jusqu'ici : une Mission datée par la base
    // divergerait de la trace d'audit, qui l'est par l'application.
    let pool = pool().await;
    let missions = PgMissionRepository::new(pool.clone());
    let d = demande(&pool).await;
    let p = prestataire(&pool, "horodatage").await;

    let instant = Utc::now() - Duration::seconds(30);
    let ResultatAttribution::Attribuee(mission) =
        missions.attribuer(d.id, p.id, instant).await.unwrap()
    else {
        panic!("attendu une attribution");
    };
    let relue = missions
        .en_cours_pour(p.id)
        .await
        .unwrap()
        .expect("mission");
    assert_eq!(relue.cree_le.timestamp(), instant.timestamp());
    assert_eq!(relue.id, mission.id);
}
