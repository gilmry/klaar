//! Story 4.9 — les avis temps réel partent bien de la base, et au bon moment.
//!
//! **C'est le seul endroit où ce placement se teste.** La garantie n'est pas
//! « le code appelle `pg_notify` » mais « PostgreSQL ne délivre l'avis qu'au
//! `COMMIT` ». Un double en mémoire dirait ce qu'on lui a fait dire ; ici, une
//! vraie écoute est ouverte pendant qu'une vraie transaction s'écrit.

use std::time::Duration;

use chrono::Utc;
use klaar_application::ports::demande_repository::DemandeRepository;
use klaar_application::ports::devis_repository::DevisRepository;
use klaar_application::ports::evenements::{EvenementMission, GenreEvenement, CANAL};
use klaar_application::ports::mission_repository::{MissionRepository, ResultatAttribution};
use klaar_application::ports::provider_repository::ProviderRepository;
use klaar_catalog::CodeCatalogue;
use klaar_identity::{
    EmpreinteMotDePasse, MotDePasse, NumeroBce, ParametresArgon2, PreuveKyc, Provider,
};
use klaar_intervention::{Mission, StatutMission};
use klaar_matching::{Demande, Urgence};
use klaar_payment::{Devis, Proposition, DEVIS_MAX_PAR_MISSION};
use klaar_shared_kernel::{Email, Geo};
use klaar_sqlx_repos::demonstration::compte_actif_de_demonstration;
use klaar_sqlx_repos::{
    creer_pool, PgDemandeRepository, PgDevisRepository, PgMissionRepository, PgProviderRepository,
    PoolPg,
};
use sqlx::postgres::PgListener;
use uuid::Uuid;

/// Grand-Place.
const CENTRE: (f64, f64) = (50.8467, 4.3525);

/// Au-delà, l'avis n'est pas « en retard » mais absent.
const ATTENTE_MAX: Duration = Duration::from_secs(10);

fn url() -> String {
    std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI")
}

async fn pool() -> PoolPg {
    creer_pool(&url()).await.expect("connexion PostgreSQL")
}

fn numero() -> NumeroBce {
    let corps = 1_000_000 + (Uuid::new_v4().as_u128() as u64) % 8_999_999;
    NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).expect("numéro construit")
}

async fn compte(pool: &PoolPg, marqueur: &str) -> Uuid {
    let email = Email::parse(&format!("evt-{marqueur}-{}@example.eu", Uuid::new_v4())).unwrap();
    let empreinte = EmpreinteMotDePasse::calculer(
        &MotDePasse::parse("Marie@2026Secure").unwrap(),
        ParametresArgon2::tests(),
    )
    .unwrap();
    compte_actif_de_demonstration(pool, &email, &empreinte)
        .await
        .expect("compte de test")
}

async fn mission(pool: &PoolPg, marqueur: &str) -> (Mission, Provider) {
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
    PgProviderRepository::new(pool.clone())
        .creer(&p)
        .await
        .expect("création");

    let issue = PgMissionRepository::new(pool.clone())
        .attribuer(d.id, p.id, Utc::now())
        .await
        .expect("attribution");
    let ResultatAttribution::Attribuee(m) = issue else {
        panic!("attendu une attribution, obtenu {issue:?}");
    };
    (m, p)
}

/// Ouvre une écoute et rend les avis **de cette Mission**, les autres ignorés.
///
/// Le filtre est indispensable : la base de développement est partagée avec le
/// reste de la suite, et un autre cas exécuté en parallèle publie sur le même
/// canal. Attendre « le prochain avis » rendrait ce test intermittent.
async fn attendre(
    ecouteur: &mut PgListener,
    mission_id: Uuid,
    combien: usize,
) -> Vec<EvenementMission> {
    let mut recus = Vec::new();
    let issue = tokio::time::timeout(ATTENTE_MAX, async {
        while recus.len() < combien {
            let avis = ecouteur.recv().await.expect("écoute");
            if let Some(e) = EvenementMission::depuis_json(avis.payload()) {
                if e.mission_id == mission_id {
                    recus.push(e);
                }
            }
        }
    })
    .await;
    assert!(
        issue.is_ok(),
        "{combien} avis attendus, {} reçus avant expiration",
        recus.len()
    );
    recus
}

async fn ecoute() -> PgListener {
    let mut ecouteur = PgListener::connect(&url()).await.expect("écoute");
    ecouteur.listen(CANAL).await.expect("abonnement au canal");
    ecouteur
}

// === @happy ===

#[tokio::test]
async fn happy_une_transition_de_mission_produit_un_avis() {
    let pool = pool().await;
    let (mut m, _) = mission(&pool, "transition").await;
    let missions = PgMissionRepository::new(pool.clone());
    let mut ecouteur = ecoute().await;

    let entree = m
        .transiter(StatutMission::EnRoute, None, None, Utc::now())
        .expect("transition permise");
    assert!(missions
        .transiter(m.id, StatutMission::Acceptee, &entree)
        .await
        .expect("écriture"));

    let recus = attendre(&mut ecouteur, m.id, 1).await;
    assert_eq!(recus[0].genre, GenreEvenement::StatutMission);
    assert_eq!(recus[0].statut.as_deref(), Some("PROVIDER_EN_ROUTE"));
}

#[tokio::test]
async fn happy_un_devis_emis_produit_un_avis() {
    let pool = pool().await;
    let (m, p) = mission(&pool, "devis").await;
    let depot = PgDevisRepository::new(pool.clone());
    let mut ecouteur = ecoute().await;

    let devis = Devis::emettre(
        m.id,
        p.id,
        Proposition {
            montant_htva_cents: 18_000,
            taux_tva_bp: 2100,
            delai_minutes: 45,
            note: None,
            preuve_tva_reduite: None,
        },
        Utc::now(),
    )
    .expect("devis valide");
    depot
        .emettre(&devis, DEVIS_MAX_PAR_MISSION)
        .await
        .expect("émission");

    let recus = attendre(&mut ecouteur, m.id, 1).await;
    assert_eq!(recus[0].genre, GenreEvenement::DevisEmis);
    assert_eq!(recus[0].statut, None);
}

// === @negative ===

#[tokio::test]
async fn negative_une_transition_perdue_n_annonce_rien() {
    // La garde sur le statut de départ refuse la seconde transition : la
    // transaction est défaite, donc l'avis ne part pas. C'est PostgreSQL qui le
    // garantit, pas une précaution dans le code appelant.
    let pool = pool().await;
    let (mut m, _) = mission(&pool, "perdue").await;
    let missions = PgMissionRepository::new(pool.clone());

    let entree = m
        .transiter(StatutMission::EnRoute, None, None, Utc::now())
        .expect("transition permise");
    assert!(missions
        .transiter(m.id, StatutMission::Acceptee, &entree)
        .await
        .unwrap());

    let mut ecouteur = ecoute().await;
    // Rejouée depuis un état qu'elle a quitté : refusée.
    assert!(!missions
        .transiter(m.id, StatutMission::Acceptee, &entree)
        .await
        .unwrap());

    let silence = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let avis = ecouteur.recv().await.expect("écoute");
            if let Some(e) = EvenementMission::depuis_json(avis.payload()) {
                if e.mission_id == m.id {
                    return e;
                }
            }
        }
    })
    .await;
    assert!(silence.is_err(), "aucun avis ne doit suivre un refus");
}

#[tokio::test]
async fn negative_un_devis_refuse_n_annonce_rien() {
    let pool = pool().await;
    let (m, p) = mission(&pool, "refuse").await;
    let depot = PgDevisRepository::new(pool.clone());

    fn devis(mission_id: Uuid, provider_id: Uuid) -> Devis {
        Devis::emettre(
            mission_id,
            provider_id,
            Proposition {
                montant_htva_cents: 18_000,
                taux_tva_bp: 2100,
                delai_minutes: 45,
                note: None,
                preuve_tva_reduite: None,
            },
            Utc::now(),
        )
        .expect("devis valide")
    }

    depot
        .emettre(&devis(m.id, p.id), DEVIS_MAX_PAR_MISSION)
        .await
        .expect("premier devis");

    let mut ecouteur = ecoute().await;
    // Un second devis pendant que le premier attend : refusé, donc muet.
    depot
        .emettre(&devis(m.id, p.id), DEVIS_MAX_PAR_MISSION)
        .await
        .expect("appel abouti");

    let silence = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let avis = ecouteur.recv().await.expect("écoute");
            if let Some(e) = EvenementMission::depuis_json(avis.payload()) {
                if e.mission_id == m.id {
                    return e;
                }
            }
        }
    })
    .await;
    assert!(silence.is_err(), "un devis refusé ne s'annonce pas");
}

// === @edge ===

#[tokio::test]
async fn edge_le_balayage_annonce_chaque_devis_expire() {
    let pool = pool().await;
    let (m, p) = mission(&pool, "expire").await;
    let depot = PgDevisRepository::new(pool.clone());

    let mut devis = Devis::emettre(
        m.id,
        p.id,
        Proposition {
            montant_htva_cents: 18_000,
            taux_tva_bp: 2100,
            delai_minutes: 45,
            note: None,
            preuve_tva_reduite: None,
        },
        Utc::now(),
    )
    .expect("devis valide");
    devis.cree_le = Utc::now() - chrono::Duration::hours(2);
    devis.expire_le = devis.cree_le + chrono::Duration::minutes(60);
    depot
        .emettre(&devis, DEVIS_MAX_PAR_MISSION)
        .await
        .expect("émission");

    let mut ecouteur = ecoute().await;
    // Le balayage est borné : plusieurs passages peuvent être nécessaires sur
    // une base partagée, où d'autres cas ont laissé des devis échus.
    for _ in 0..10 {
        let eteints = depot.expirer_les_echus(Utc::now(), 500).await.unwrap();
        if eteints.iter().any(|d| d.id == devis.id) || eteints.is_empty() {
            break;
        }
    }

    let recus = attendre(&mut ecouteur, m.id, 1).await;
    assert_eq!(recus[0].genre, GenreEvenement::DevisExpire);
}

// === @security ===

#[tokio::test]
async fn security_l_avis_ne_porte_ni_adresse_ni_montant() {
    // La charge d'un `NOTIFY` traverse la base et ses journaux, et part vers
    // tous les exemplaires du service. Ce test lit la charge brute, avant tout
    // décodage : c'est elle qui voyage.
    let pool = pool().await;
    let (m, p) = mission(&pool, "discret").await;
    let depot = PgDevisRepository::new(pool.clone());
    let mut ecouteur = ecoute().await;

    let devis = Devis::emettre(
        m.id,
        p.id,
        Proposition {
            montant_htva_cents: 18_000,
            taux_tva_bp: 2100,
            delai_minutes: 45,
            note: Some("remplacement joint".to_string()),
            preuve_tva_reduite: None,
        },
        Utc::now(),
    )
    .expect("devis valide");
    depot
        .emettre(&devis, DEVIS_MAX_PAR_MISSION)
        .await
        .expect("émission");

    let charge = tokio::time::timeout(ATTENTE_MAX, async {
        loop {
            let avis = ecouteur.recv().await.expect("écoute");
            let brut = avis.payload().to_string();
            if brut.contains(&m.id.to_string()) {
                return brut;
            }
        }
    })
    .await
    .expect("un avis");

    for interdit in ["18000", "217", "remplacement", "50.84", "4.35", "@example"] {
        assert!(
            !charge.contains(interdit),
            "« {interdit} » n'a rien à faire dans la charge : {charge}"
        );
    }
}
