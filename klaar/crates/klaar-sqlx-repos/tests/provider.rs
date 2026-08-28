//! Story 1.6 (partielle) — dépôt des prestataires, contre un vrai PostgreSQL.
//!
//! La recherche par rayon est du SQL spatial : ni le domaine ni un double en
//! mémoire ne diraient si `ST_DWithin` mesure ce qu'on croit, ni si l'ordre des
//! arguments de `ST_MakePoint` est le bon.

use chrono::Utc;
use klaar_application::ports::provider_repository::ProviderRepository;
use klaar_catalog::CodeCatalogue;
use klaar_identity::{
    EmpreinteMotDePasse, MotDePasse, NumeroBce, OrigineKyc, ParametresArgon2, PreuveKyc, Provider,
};
use klaar_shared_kernel::{Email, Geo};
use klaar_sqlx_repos::demonstration::compte_actif_de_demonstration;
use klaar_sqlx_repos::{creer_pool, PgProviderRepository, PoolPg};
use uuid::Uuid;

/// Grand-Place.
const CENTRE: (f64, f64) = (50.8467, 4.3525);

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

/// Numéro BCE construit, jamais copié d'une entreprise réelle, et **différent à
/// chaque appel**.
///
/// Un numéro déterministe entrait en collision avec les lignes laissées par
/// l'exécution précédente : la base garde ses prestataires d'un `cargo test` à
/// l'autre, et l'unicité du numéro BCE est justement ce qu'on veut imposer. Le
/// tirer au sort rend la suite rejouable sans nettoyage préalable.
fn numero() -> NumeroBce {
    // Le corps reste sous dix millions : formaté sur huit chiffres, il porte
    // alors un zéro de tête, et un numéro d'entreprise commence par 0 ou 1.
    // Un tirage plus large produisait des préfixes 2 à 8, que le domaine refuse
    // à juste titre.
    let corps = 1_000_000 + (Uuid::new_v4().as_u128() as u64) % 8_999_999;
    NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).expect("numéro construit")
}

fn secteur(code: &str) -> CodeCatalogue {
    CodeCatalogue::parse(code).unwrap()
}

async fn compte(pool: &PoolPg, marqueur: &str) -> Uuid {
    let email = Email::parse(&format!("prov-{marqueur}-{}@example.eu", Uuid::new_v4())).unwrap();
    let empreinte = EmpreinteMotDePasse::calculer(
        &MotDePasse::parse("Marie@2026Secure").unwrap(),
        ParametresArgon2::tests(),
    )
    .unwrap();
    compte_actif_de_demonstration(pool, &email, &empreinte)
        .await
        .expect("compte de test")
}

/// Crée un prestataire actif et disponible, et le rend.
///
/// Rend le `Provider` complet et non son seul identifiant : le cas qui vérifie
/// l'unicité du numéro BCE a besoin de réutiliser celui qui vient d'être posé.
async fn prestataire(
    depot: &PgProviderRepository,
    pool: &PoolPg,
    marqueur: &str,
    lat: f64,
    lon: f64,
    secteurs: &[&str],
) -> Provider {
    let utilisateur_id = compte(pool, marqueur).await;
    let mut p = Provider::inscrire(
        utilisateur_id,
        numero(),
        &format!("Prestataire {marqueur}"),
        Geo::new(lat, lon).unwrap(),
        secteurs.iter().map(|s| secteur(s)).collect(),
        Utc::now(),
    )
    .expect("prestataire valide");
    p.valider_kyc(PreuveKyc::demonstration(Utc::now()));
    depot.creer(&p).await.expect("création");
    depot
        .definir_disponibilite(p.id, true)
        .await
        .expect("disponibilité");
    p
}

#[tokio::test]
async fn happy_un_prestataire_cree_se_relit_avec_ses_competences() {
    let pool = pool().await;
    let depot = PgProviderRepository::new(pool.clone());
    let cible = prestataire(
        &depot,
        &pool,
        "relecture",
        CENTRE.0,
        CENTRE.1,
        &["plomberie", "serrurerie"],
    )
    .await;

    let relu = depot
        .par_id(cible.id)
        .await
        .unwrap()
        .expect("le prestataire");
    assert_eq!(relu.statut.as_str(), "ACTIVE");
    assert_eq!(relu.origine_kyc, Some(OrigineKyc::Demonstration));
    assert_eq!(relu.competences.len(), 2);
    assert!(relu.couvre(&secteur("plomberie")));
    // La position relue doit être celle écrite : `ST_MakePoint` prend la
    // longitude d'abord, et l'inverser place Bruxelles au large de la Somalie.
    assert!((relu.base.lat() - CENTRE.0).abs() < 1e-6);
    assert!((relu.base.lon() - CENTRE.1).abs() < 1e-6);
}

#[tokio::test]
async fn happy_la_recherche_rend_les_prestataires_du_secteur_tries_par_distance() {
    let pool = pool().await;
    let depot = PgProviderRepository::new(pool.clone());
    // Trois prestataires du même secteur, à des distances croissantes.
    let proche = prestataire(&depot, &pool, "tri-a", CENTRE.0, CENTRE.1, &["auto"]).await;
    let moyen = prestataire(
        &depot,
        &pool,
        "tri-b",
        CENTRE.0 + 0.009,
        CENTRE.1,
        &["auto"],
    )
    .await;
    let loin = prestataire(
        &depot,
        &pool,
        "tri-c",
        CENTRE.0 + 0.027,
        CENTRE.1,
        &["auto"],
    )
    .await;

    let trouves = depot
        .proches(
            &secteur("auto"),
            Geo::new(CENTRE.0, CENTRE.1).unwrap(),
            5_000.0,
            500,
        )
        .await
        .unwrap();

    // Le classement est vérifié **entre les trois prestataires de ce cas**, et
    // non sur la liste entière : la base conserve ceux des exécutions
    // précédentes, et présumer un jeu de données restreint rend le test faux
    // au bout de quelques lancements. La limite est relevée pour la même
    // raison — sinon les trois pourraient être tronqués hors du résultat.
    let miens: Vec<&klaar_application::ports::provider_repository::ProviderProche> = trouves
        .iter()
        .filter(|t| [proche.id, moyen.id, loin.id].contains(&t.provider.id))
        .collect();
    assert_eq!(miens.len(), 3, "les trois doivent être trouvés");

    let ids: Vec<Uuid> = miens.iter().map(|p| p.provider.id).collect();
    assert_eq!(
        ids,
        vec![proche.id, moyen.id, loin.id],
        "du plus proche au plus loin"
    );

    // Les distances sont croissantes et en mètres.
    let distances: Vec<f64> = miens.iter().map(|p| p.distance_metres).collect();
    assert!(distances.windows(2).all(|f| f[0] <= f[1]), "{distances:?}");
    assert!(
        distances[0] < 10.0,
        "le premier est sur place : {distances:?}"
    );
    assert!(
        distances[2] > 2_000.0,
        "le troisième est à environ 3 km : {distances:?}"
    );
}

#[tokio::test]
async fn negative_un_prestataire_hors_rayon_n_est_pas_rendu() {
    let pool = pool().await;
    let depot = PgProviderRepository::new(pool.clone());
    prestataire(
        &depot,
        &pool,
        "rayon-loin",
        // Environ 8 km au nord : hors d'un rayon de 5 km.
        CENTRE.0 + 0.072,
        CENTRE.1,
        &["livraison"],
    )
    .await;

    let trouves = depot
        .proches(
            &secteur("livraison"),
            Geo::new(CENTRE.0, CENTRE.1).unwrap(),
            5_000.0,
            20,
        )
        .await
        .unwrap();
    assert!(
        trouves.iter().all(|p| p.distance_metres <= 5_000.0),
        "un prestataire hors rayon a été rendu"
    );
}

#[tokio::test]
async fn negative_un_prestataire_d_un_autre_secteur_n_est_pas_rendu() {
    let pool = pool().await;
    let depot = PgProviderRepository::new(pool.clone());
    let cible = prestataire(
        &depot,
        &pool,
        "secteur",
        CENTRE.0,
        CENTRE.1,
        &["serrurerie"],
    )
    .await;

    let trouves = depot
        .proches(
            &secteur("electricite"),
            Geo::new(CENTRE.0, CENTRE.1).unwrap(),
            5_000.0,
            20,
        )
        .await
        .unwrap();
    assert!(!trouves.iter().any(|p| p.provider.id == cible.id));
}

#[tokio::test]
async fn edge_un_prestataire_multi_secteur_n_apparait_qu_une_fois() {
    // Le filtre de compétence est un `EXISTS` et non une jointure : joindre
    // dupliquerait la ligne du prestataire par compétence, et la limite
    // porterait sur les couples plutôt que sur les prestataires.
    let pool = pool().await;
    let depot = PgProviderRepository::new(pool.clone());
    let cible = prestataire(
        &depot,
        &pool,
        "multi",
        CENTRE.0,
        CENTRE.1,
        &["plomberie", "serrurerie", "auto", "livraison"],
    )
    .await;

    let trouves = depot
        .proches(
            &secteur("plomberie"),
            Geo::new(CENTRE.0, CENTRE.1).unwrap(),
            5_000.0,
            // Large, et non cinquante : la base garde les prestataires des
            // exécutions précédentes, tous posés au même point. Avec une limite
            // serrée, la cible finissait par tomber hors des premiers rendus et
            // le cas échouait sans qu'aucune duplication n'ait eu lieu.
            1_000,
        )
        .await
        .unwrap();
    assert_eq!(
        trouves.iter().filter(|p| p.provider.id == cible.id).count(),
        1,
        "un prestataire multi-secteur ne doit apparaître qu'une fois"
    );
}

#[tokio::test]
async fn edge_la_limite_borne_le_nombre_de_prestataires_rendus() {
    let pool = pool().await;
    let depot = PgProviderRepository::new(pool.clone());
    for i in 0..4 {
        prestataire(
            &depot,
            &pool,
            &format!("limite-{i}"),
            CENTRE.0 + i as f64 * 0.001,
            CENTRE.1,
            &["electricite"],
        )
        .await;
    }

    let trouves = depot
        .proches(
            &secteur("electricite"),
            Geo::new(CENTRE.0, CENTRE.1).unwrap(),
            5_000.0,
            2,
        )
        .await
        .unwrap();
    assert_eq!(trouves.len(), 2);
}

#[tokio::test]
async fn security_un_prestataire_en_attente_de_kyc_n_est_jamais_sollicite() {
    // Le coeur du dispositif : sans contrôle, pas de Demande. C'est ce que la
    // recherche doit imposer, pas seulement le domaine.
    let pool = pool().await;
    let depot = PgProviderRepository::new(pool.clone());
    let utilisateur_id = compte(&pool, "attente").await;
    let p = Provider::inscrire(
        utilisateur_id,
        numero(),
        "Prestataire non contrôlé",
        Geo::new(CENTRE.0, CENTRE.1).unwrap(),
        vec![secteur("plomberie")],
        Utc::now(),
    )
    .unwrap();
    depot.creer(&p).await.unwrap();
    // Disponible, mais pas actif : la disponibilité seule ne suffit pas.
    depot.definir_disponibilite(p.id, true).await.unwrap();

    let trouves = depot
        .proches(
            &secteur("plomberie"),
            Geo::new(CENTRE.0, CENTRE.1).unwrap(),
            5_000.0,
            50,
        )
        .await
        .unwrap();
    assert!(!trouves.iter().any(|t| t.provider.id == p.id));
}

#[tokio::test]
async fn security_un_prestataire_indisponible_n_est_pas_sollicite() {
    // Être actif et être disponible sont deux notions distinctes : les
    // confondre ferait de « je suis en congé » une radiation.
    let pool = pool().await;
    let depot = PgProviderRepository::new(pool.clone());
    let cible = prestataire(&depot, &pool, "conge", CENTRE.0, CENTRE.1, &["auto"]).await;
    depot.definir_disponibilite(cible.id, false).await.unwrap();

    let trouves = depot
        .proches(
            &secteur("auto"),
            Geo::new(CENTRE.0, CENTRE.1).unwrap(),
            5_000.0,
            50,
        )
        .await
        .unwrap();
    assert!(!trouves.iter().any(|p| p.provider.id == cible.id));
}

#[tokio::test]
async fn security_un_prestataire_suspendu_sort_du_matching() {
    let pool = pool().await;
    let depot = PgProviderRepository::new(pool.clone());
    let cible = prestataire(
        &depot,
        &pool,
        "suspendu",
        CENTRE.0,
        CENTRE.1,
        &["serrurerie"],
    )
    .await;

    let mut p = depot.par_id(cible.id).await.unwrap().unwrap();
    p.suspendre();
    depot.mettre_a_jour_etat(&p).await.unwrap();

    let trouves = depot
        .proches(
            &secteur("serrurerie"),
            Geo::new(CENTRE.0, CENTRE.1).unwrap(),
            5_000.0,
            50,
        )
        .await
        .unwrap();
    assert!(!trouves.iter().any(|t| t.provider.id == cible.id));

    // L'origine du contrôle survit à la suspension : elle dit comment il a été
    // activé, pas s'il l'est encore.
    let relu = depot.par_id(cible.id).await.unwrap().unwrap();
    assert_eq!(relu.origine_kyc, Some(OrigineKyc::Demonstration));
}

#[tokio::test]
async fn security_la_base_refuse_un_prestataire_actif_sans_origine_de_controle() {
    // La contrainte est reposée par la base : aucun chemin d'écriture, même
    // direct, ne peut activer un prestataire sans dire d'où vient son contrôle.
    let pool = pool().await;
    let utilisateur_id = compte(&pool, "sans-origine").await;
    let erreur = sqlx::query(
        "INSERT INTO provider
             (id, utilisateur_id, numero_bce, raison_sociale, base, statut, origine_kyc, cree_le)
         VALUES ($1, $2, $3, 'Sans contrôle',
                 ST_SetSRID(ST_MakePoint(4.35, 50.84), 4326)::geography,
                 'ACTIVE', NULL, now())",
    )
    .bind(Uuid::new_v4())
    .bind(utilisateur_id)
    .bind(numero().as_str())
    .execute(&pool)
    .await;
    assert!(erreur.is_err(), "un actif sans origine doit être refusé");
}

#[tokio::test]
async fn security_deux_prestataires_ne_partagent_pas_un_numero_bce() {
    // Un même numéro d'entreprise sur deux fiches permettrait à quelqu'un de se
    // dédoubler pour contourner une suspension.
    let pool = pool().await;
    let depot = PgProviderRepository::new(pool.clone());
    let premier = prestataire(&depot, &pool, "bce-a", CENTRE.0, CENTRE.1, &["plomberie"]).await;

    let utilisateur_id = compte(&pool, "bce-b").await;
    let mut double = Provider::inscrire(
        utilisateur_id,
        premier.numero_bce.clone(),
        "Le même numéro",
        Geo::new(CENTRE.0, CENTRE.1).unwrap(),
        vec![secteur("plomberie")],
        Utc::now(),
    )
    .unwrap();
    double.valider_kyc(PreuveKyc::demonstration(Utc::now()));
    assert!(depot.creer(&double).await.is_err());
}
