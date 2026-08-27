//! Story 2.1 — le catalogue amorcé, lu depuis un vrai PostgreSQL.
//!
//! Ces cas portent sur le **jeu de données** autant que sur le code. Un
//! catalogue dont une entrée n'est pas traduite se voit à l'usage, pas à la
//! relecture d'un fichier SQL de quatre-vingts lignes.

use klaar_application::ports::catalogue_repository::CatalogueRepository;
use klaar_shared_kernel::Locale;
use klaar_sqlx_repos::{creer_pool, PgCatalogueRepository};

/// Les cinq secteurs que le PRD nomme (FR-008, `Background`).
const SECTEURS_MVP: [&str; 5] = [
    "plomberie",
    "serrurerie",
    "electricite",
    "auto",
    "livraison",
];

async fn depot() -> PgCatalogueRepository {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    PgCatalogueRepository::new(creer_pool(&url).await.expect("connexion PostgreSQL"))
}

#[tokio::test]
async fn happy_le_catalogue_contient_les_cinq_secteurs_du_mvp() {
    let secteurs = depot().await.secteurs().await.unwrap();
    let codes: Vec<&str> = secteurs.iter().map(|s| s.code.as_str()).collect();
    assert_eq!(codes, SECTEURS_MVP);
}

#[tokio::test]
async fn happy_chaque_secteur_liste_ses_skills() {
    let secteurs = depot().await.secteurs().await.unwrap();
    for secteur in &secteurs {
        assert!(
            !secteur.skills.is_empty(),
            "le secteur {} n'a aucun Skill",
            secteur.code
        );
    }
}

#[tokio::test]
async fn happy_l_ordre_d_affichage_est_stable() {
    // Explicite plutôt qu'alphabétique : l'ordre alphabétique change d'une
    // langue à l'autre, et le catalogue apparaîtrait dans un ordre différent
    // selon la langue choisie.
    let premier = depot().await.secteurs().await.unwrap();
    let second = depot().await.secteurs().await.unwrap();
    assert_eq!(
        premier.iter().map(|s| s.code.clone()).collect::<Vec<_>>(),
        second.iter().map(|s| s.code.clone()).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn negative_aucun_code_du_catalogue_n_est_illisible() {
    // Le domaine refuse majuscules, accents et ponctuation ; la base pose la
    // même règle en `CHECK`. Ce test vérifie que les deux disent la même chose
    // — sans quoi une entrée valide en base ferait échouer la lecture.
    let secteurs = depot().await.secteurs().await.unwrap();
    for secteur in &secteurs {
        assert!(klaar_catalog::CodeCatalogue::parse(secteur.code.as_str()).is_ok());
        for skill in &secteur.skills {
            assert!(klaar_catalog::CodeCatalogue::parse(skill.code.as_str()).is_ok());
        }
    }
}

#[tokio::test]
async fn edge_aucun_code_de_skill_n_apparait_deux_fois() {
    // Deux entrées identiques à l'affichage, comptées séparément dans les
    // statistiques : le genre de défaut qu'on ne voit qu'au rapport annuel.
    let secteurs = depot().await.secteurs().await.unwrap();
    let mut tous: Vec<String> = secteurs
        .iter()
        .flat_map(|s| s.skills.iter().map(|k| k.code.to_string()))
        .collect();
    let avant = tous.len();
    tous.sort();
    tous.dedup();
    assert_eq!(tous.len(), avant, "un code de Skill apparaît deux fois");
}

#[tokio::test]
async fn edge_chaque_secteur_est_coherent_selon_le_domaine() {
    let secteurs = depot().await.secteurs().await.unwrap();
    for secteur in &secteurs {
        assert!(secteur.coherent(), "secteur incohérent : {}", secteur.code);
    }
}

#[tokio::test]
async fn security_tout_est_traduit_dans_les_trois_langues() {
    // Bruxelles est officiellement bilingue : une entrée sans néerlandais n'est
    // pas une entrée incomplète, c'est une entrée qui ne devrait pas exister.
    let secteurs = depot().await.secteurs().await.unwrap();
    for secteur in &secteurs {
        for locale in [Locale::Fr, Locale::Nl, Locale::En] {
            assert!(
                !secteur.libelles.pour(locale).trim().is_empty(),
                "secteur {} sans libellé {}",
                secteur.code,
                locale.as_str()
            );
            for skill in &secteur.skills {
                assert!(
                    !skill.libelles.pour(locale).trim().is_empty(),
                    "skill {} sans libellé {}",
                    skill.code,
                    locale.as_str()
                );
            }
        }
    }
}

#[tokio::test]
async fn security_les_traductions_ne_sont_pas_de_simples_copies_du_francais() {
    // Une traduction recopiée du français est le symptôme habituel d'un jeu de
    // données « à compléter plus tard ». Quelques mots sont légitimement
    // identiques d'une langue à l'autre — « Auto » l'est en français comme en
    // néerlandais — d'où un seuil plutôt qu'une interdiction.
    let secteurs = depot().await.secteurs().await.unwrap();
    let entrees: Vec<(&str, &str)> = secteurs
        .iter()
        .map(|s| (s.libelles.fr.as_str(), s.libelles.nl.as_str()))
        .chain(
            secteurs
                .iter()
                .flat_map(|s| &s.skills)
                .map(|k| (k.libelles.fr.as_str(), k.libelles.nl.as_str())),
        )
        .collect();

    let copies = entrees.iter().filter(|(fr, nl)| fr == nl).count();
    assert!(
        copies * 10 < entrees.len(),
        "{copies} entrées sur {} ont un néerlandais identique au français",
        entrees.len()
    );
}
