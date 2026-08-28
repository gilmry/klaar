//! Story 4.1 — comptages et balayage des devis, contre un vrai PostgreSQL.
//!
//! **C'est le seul endroit où ces règles se testent.** « Un seul devis en
//! attente » est un index partiel, « trois au maximum » est un `WHERE` sur
//! l'insertion, et l'extinction des devis échus est une seule instruction. Un
//! double en mémoire dirait ce qu'on lui a fait dire ; ici, ce sont de vraies
//! écritures concurrentes sur des connexions distinctes.

use chrono::{Duration, Utc};
use klaar_application::ports::demande_repository::DemandeRepository;
use klaar_application::ports::devis_repository::{DevisRepository, ResultatEmission};
use klaar_application::ports::mission_repository::MissionRepository;
use klaar_application::ports::provider_repository::ProviderRepository;
use klaar_catalog::CodeCatalogue;
use klaar_identity::{
    EmpreinteMotDePasse, MotDePasse, NumeroBce, ParametresArgon2, PreuveKyc, Provider,
};
use klaar_matching::{Demande, Urgence};
use klaar_payment::{Devis, Proposition, StatutDevis, DEVIS_MAX_PAR_MISSION};
use klaar_shared_kernel::{Email, Geo};
use klaar_sqlx_repos::demonstration::compte_actif_de_demonstration;
use klaar_sqlx_repos::{
    creer_pool, PgDemandeRepository, PgDevisRepository, PgMissionRepository, PgProviderRepository,
    PoolPg,
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
    let corps = 1_000_000 + (Uuid::new_v4().as_u128() as u64) % 8_999_999;
    NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).expect("numéro construit")
}

async fn compte(pool: &PoolPg, marqueur: &str) -> Uuid {
    let email = Email::parse(&format!("devis-{marqueur}-{}@example.eu", Uuid::new_v4())).unwrap();
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

/// Une Mission attribuée, prête à recevoir des devis.
async fn mission(pool: &PoolPg, marqueur: &str) -> (Uuid, Provider) {
    let demandes = PgDemandeRepository::new(pool.clone());
    let d = Demande::soumettre(
        compte(pool, "demandeur").await,
        CodeCatalogue::parse("plomberie").unwrap(),
        "Fuite sous l'évier",
        Geo::new(CENTRE.0, CENTRE.1).unwrap(),
        Urgence::Haute,
        Utc::now(),
    )
    .expect("Demande valide");
    demandes.creer(&d).await.expect("création");

    let p = prestataire(pool, marqueur).await;
    let missions = PgMissionRepository::new(pool.clone());
    let issue = missions
        .attribuer(d.id, p.id, Utc::now())
        .await
        .expect("attribution");
    let klaar_application::ports::mission_repository::ResultatAttribution::Attribuee(m) = issue
    else {
        panic!("attendu une attribution, obtenu {issue:?}");
    };
    (m.id, p)
}

fn proposition() -> Proposition {
    Proposition {
        montant_htva_cents: 18_000,
        taux_tva_bp: 2100,
        delai_minutes: 45,
        note: None,
        preuve_tva_reduite: None,
    }
}

fn devis_de(mission_id: Uuid, provider_id: Uuid) -> Devis {
    Devis::emettre(mission_id, provider_id, proposition(), Utc::now()).expect("devis valide")
}

/// Clôt le devis en attente, comme le fera le demandeur (FR-017).
async fn refuser(pool: &PoolPg, mission_id: Uuid) {
    sqlx::query("UPDATE devis SET statut = 'REFUSED' WHERE mission_id = $1 AND statut = 'SENT'")
        .bind(mission_id)
        .execute(pool)
        .await
        .expect("refus");
}

// === @happy ===

#[tokio::test]
async fn happy_un_devis_ecrit_se_relit_a_l_identique() {
    let pool = pool().await;
    let depot = PgDevisRepository::new(pool.clone());
    let (mission_id, p) = mission(&pool, "relecture").await;

    let devis = devis_de(mission_id, p.id);
    let issue = depot.emettre(&devis, DEVIS_MAX_PAR_MISSION).await.unwrap();
    assert!(matches!(issue, ResultatEmission::Emis(_)));

    let relu = depot
        .en_cours_pour_mission(mission_id)
        .await
        .unwrap()
        .expect("le devis");
    assert_eq!(relu.id, devis.id);
    assert_eq!(relu.montant_htva.cents(), 18_000);
    assert_eq!(relu.tva.cents(), 3_780);
    assert_eq!(relu.total_ttc.cents(), 21_780);
    assert_eq!(relu.taux_tva.basis_points(), 2100);
    assert_eq!(relu.delai_minutes, 45);
    assert_eq!(relu.statut, StatutDevis::Envoye);
    assert_eq!(relu.provider_id, p.id);
}

// === @negative ===

#[tokio::test]
async fn negative_un_second_devis_en_attente_est_refuse() {
    let pool = pool().await;
    let depot = PgDevisRepository::new(pool.clone());
    let (mission_id, p) = mission(&pool, "double").await;

    depot
        .emettre(&devis_de(mission_id, p.id), DEVIS_MAX_PAR_MISSION)
        .await
        .unwrap();
    let second = depot
        .emettre(&devis_de(mission_id, p.id), DEVIS_MAX_PAR_MISSION)
        .await
        .unwrap();

    assert_eq!(second, ResultatEmission::DejaEnCours);
    assert_eq!(depot.compter_pour_mission(mission_id).await.unwrap(), 1);
}

// === @edge ===

#[tokio::test]
async fn edge_le_plafond_de_trois_devis_tient() {
    let pool = pool().await;
    let depot = PgDevisRepository::new(pool.clone());
    let (mission_id, p) = mission(&pool, "plafond").await;

    for tour in 1..=DEVIS_MAX_PAR_MISSION {
        let issue = depot
            .emettre(&devis_de(mission_id, p.id), DEVIS_MAX_PAR_MISSION)
            .await
            .unwrap();
        assert!(matches!(issue, ResultatEmission::Emis(_)), "devis {tour}");
        refuser(&pool, mission_id).await;
    }

    let quatrieme = depot
        .emettre(&devis_de(mission_id, p.id), DEVIS_MAX_PAR_MISSION)
        .await
        .unwrap();
    assert_eq!(quatrieme, ResultatEmission::PlafondAtteint);
    assert_eq!(
        depot.compter_pour_mission(mission_id).await.unwrap(),
        DEVIS_MAX_PAR_MISSION
    );
}

#[tokio::test]
async fn edge_le_dernier_devis_reste_lisible_apres_refus() {
    // Le suivi du demandeur lit le dernier devis quel que soit son statut : le
    // faire disparaître laisserait l'écran vide sans dire ce qui s'est passé.
    let pool = pool().await;
    let depot = PgDevisRepository::new(pool.clone());
    let (mission_id, p) = mission(&pool, "apres-refus").await;

    let devis = devis_de(mission_id, p.id);
    depot.emettre(&devis, DEVIS_MAX_PAR_MISSION).await.unwrap();
    refuser(&pool, mission_id).await;

    assert!(depot
        .en_cours_pour_mission(mission_id)
        .await
        .unwrap()
        .is_none());
    let dernier = depot
        .dernier_pour_mission(mission_id)
        .await
        .unwrap()
        .expect("le devis refusé");
    assert_eq!(dernier.id, devis.id);
    assert_eq!(dernier.statut, StatutDevis::Refuse);
}

#[tokio::test]
async fn edge_le_balayage_eteint_les_echus_et_ne_les_rend_qu_une_fois() {
    let pool = pool().await;
    let depot = PgDevisRepository::new(pool.clone());
    let (mission_id, p) = mission(&pool, "balayage").await;

    let mut devis = devis_de(mission_id, p.id);
    // Émis il y a deux heures : au-delà de l'heure de validité.
    devis.cree_le = Utc::now() - Duration::hours(2);
    devis.expire_le = devis.cree_le + Duration::minutes(60);
    depot.emettre(&devis, DEVIS_MAX_PAR_MISSION).await.unwrap();

    // La file est vidée plutôt que balayée une fois : un passage rend les plus
    // anciens d'abord, et une base partagée avec le reste de la suite en garde
    // beaucoup.
    for _ in 0..10 {
        if depot
            .expirer_les_echus(Utc::now(), 500)
            .await
            .unwrap()
            .is_empty()
        {
            break;
        }
    }

    // **Ce qui est asserté est l'issue, pas qui l'a produite.** Un autre binaire
    // de test balaie la même table au même moment : si son passage éteint ce
    // devis avant le nôtre, le nôtre ne le rendra pas — et ce serait le
    // comportement correct, puisqu'un balayage ne rend que ce qu'il vient
    // d'éteindre. Assertée telle quelle, la propriété « il nous est rendu »
    // dépendait de ce qui tournait en parallèle.
    //
    // Que le balayage rende exactement ce qu'il éteint est vérifié là où c'est
    // isolable : `usecases::expirer_devis`, sur un double en mémoire.
    let relu = depot
        .dernier_pour_mission(mission_id)
        .await
        .unwrap()
        .expect("le devis");
    assert_eq!(relu.statut, StatutDevis::Expire);

    // Idempotence : un second passage ne le retrouve plus.
    for _ in 0..10 {
        let encore = depot.expirer_les_echus(Utc::now(), 500).await.unwrap();
        assert!(
            !encore.iter().any(|d| d.id == devis.id),
            "un devis déjà éteint ne doit pas être rendu deux fois"
        );
        if encore.is_empty() {
            break;
        }
    }
}

#[tokio::test]
async fn edge_un_devis_encore_valable_survit_au_balayage() {
    let pool = pool().await;
    let depot = PgDevisRepository::new(pool.clone());
    let (mission_id, p) = mission(&pool, "valable").await;

    let devis = devis_de(mission_id, p.id);
    depot.emettre(&devis, DEVIS_MAX_PAR_MISSION).await.unwrap();

    depot.expirer_les_echus(Utc::now(), 500).await.unwrap();

    let relu = depot
        .en_cours_pour_mission(mission_id)
        .await
        .unwrap()
        .expect("toujours en attente");
    assert_eq!(relu.statut, StatutDevis::Envoye);
}

#[tokio::test]
async fn edge_un_devis_vivant_prime_sur_le_plafond() {
    // Trois devis dont le dernier attend encore une réponse. Rendre « plafond »
    // ferait annuler la Mission par l'appelant, alors qu'une offre vivante est
    // sur la table du demandeur. C'est « déjà en cours » qu'il faut dire.
    let pool = pool().await;
    let depot = PgDevisRepository::new(pool.clone());
    let (mission_id, p) = mission(&pool, "vivant").await;

    for _ in 1..DEVIS_MAX_PAR_MISSION {
        depot
            .emettre(&devis_de(mission_id, p.id), DEVIS_MAX_PAR_MISSION)
            .await
            .unwrap();
        refuser(&pool, mission_id).await;
    }
    // Le troisième reste en attente.
    depot
        .emettre(&devis_de(mission_id, p.id), DEVIS_MAX_PAR_MISSION)
        .await
        .unwrap();
    assert_eq!(
        depot.compter_pour_mission(mission_id).await.unwrap(),
        DEVIS_MAX_PAR_MISSION
    );

    let quatrieme = depot
        .emettre(&devis_de(mission_id, p.id), DEVIS_MAX_PAR_MISSION)
        .await
        .unwrap();
    assert_eq!(quatrieme, ResultatEmission::DejaEnCours);
}

// === @security ===

#[tokio::test]
async fn security_deux_envois_simultanes_ne_posent_qu_un_devis() {
    // Le cas que l'index partiel existe pour fermer. Deux tâches, deux
    // connexions, la même Mission : sans lui, le demandeur verrait deux prix
    // sans savoir lequel l'engage.
    let pool = pool().await;
    let (mission_id, p) = mission(&pool, "course").await;
    let depot = Arc::new(PgDevisRepository::new(pool.clone()));

    let a = {
        let depot = Arc::clone(&depot);
        let devis = devis_de(mission_id, p.id);
        tokio::spawn(async move { depot.emettre(&devis, DEVIS_MAX_PAR_MISSION).await })
    };
    let b = {
        let depot = Arc::clone(&depot);
        let devis = devis_de(mission_id, p.id);
        tokio::spawn(async move { depot.emettre(&devis, DEVIS_MAX_PAR_MISSION).await })
    };

    let issues = [a.await.unwrap().unwrap(), b.await.unwrap().unwrap()];
    let emis = issues
        .iter()
        .filter(|i| matches!(i, ResultatEmission::Emis(_)))
        .count();
    assert_eq!(emis, 1, "un seul envoi doit aboutir, obtenu {issues:?}");
    assert!(issues.contains(&ResultatEmission::DejaEnCours));
    assert_eq!(depot.compter_pour_mission(mission_id).await.unwrap(), 1);
}

#[tokio::test]
async fn security_le_montant_relu_est_celui_qui_a_ete_ecrit() {
    // L'invariant §10.2 vaut jusqu'au disque : un montant modifié à l'écriture
    // ou à la relecture le violerait aussi sûrement qu'une grille tarifaire.
    let pool = pool().await;
    let depot = PgDevisRepository::new(pool.clone());

    for cents in [1_i64, 4_999, 18_000, 99_999, 1_000_000] {
        let (mission_id, p) = mission(&pool, &format!("montant-{cents}")).await;
        let devis = Devis::emettre(
            mission_id,
            p.id,
            Proposition {
                montant_htva_cents: cents,
                ..proposition()
            },
            Utc::now(),
        )
        .expect("devis valide");
        depot.emettre(&devis, DEVIS_MAX_PAR_MISSION).await.unwrap();

        let relu = depot
            .en_cours_pour_mission(mission_id)
            .await
            .unwrap()
            .expect("le devis");
        assert_eq!(relu.montant_htva.cents(), cents);
        assert_eq!(relu.total_ttc.cents(), devis.total_ttc.cents());
    }
}

#[tokio::test]
async fn security_un_devis_emis_ne_change_plus_de_prix() {
    // FR-016 `@security` demande que l'absence d'algorithme de fixation de prix
    // soit **auditable**. Un audit ne vaut que si ce qu'il lit est ce qui a été
    // présenté : le déclencheur de V20 gèle tout sauf le statut.
    let pool = pool().await;
    let depot = PgDevisRepository::new(pool.clone());
    let (mission_id, p) = mission(&pool, "fige").await;
    let devis = devis_de(mission_id, p.id);
    depot.emettre(&devis, DEVIS_MAX_PAR_MISSION).await.unwrap();

    let refus = sqlx::query("UPDATE devis SET montant_htva_cents = 1 WHERE id = $1")
        .bind(devis.id)
        .execute(&pool)
        .await
        .expect_err("le montant doit être figé");
    assert!(
        refus.to_string().contains("ne change plus de contenu"),
        "déclencheur attendu, obtenu : {refus}"
    );

    // Le statut, lui, doit rester modifiable : c'est la vie normale d'un devis.
    refuser(&pool, mission_id).await;
    let relu = depot
        .dernier_pour_mission(mission_id)
        .await
        .unwrap()
        .expect("le devis");
    assert_eq!(relu.statut, StatutDevis::Refuse);
    assert_eq!(relu.montant_htva.cents(), devis.montant_htva.cents());
}

#[tokio::test]
async fn security_le_taux_reduit_garde_sa_preuve() {
    // La contrainte de base double la règle du domaine : une écriture directe
    // en SQL ne doit pas pouvoir poser un devis à 6 % sans justification.
    let pool = pool().await;
    let (mission_id, p) = mission(&pool, "preuve").await;

    let ecrit = sqlx::query(
        "INSERT INTO devis (id, mission_id, provider_id, montant_htva_cents, taux_tva_bp,
                            tva_cents, total_ttc_cents, delai_minutes, statut, cree_le, expire_le)
         VALUES ($1, $2, $3, 18000, 600, 1080, 19080, 45, 'SENT', now(), now() + interval '1 hour')",
    )
    .bind(Uuid::new_v4())
    .bind(mission_id)
    .bind(p.id)
    .execute(&pool)
    .await;

    let erreur = ecrit.expect_err("la contrainte doit refuser un taux réduit sans preuve");
    assert!(
        erreur.to_string().contains("devis_preuve_si_taux_reduit"),
        "contrainte attendue, obtenu : {erreur}"
    );
}
