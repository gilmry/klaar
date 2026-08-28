//! Story 3.6 — balayage de fin de tour et relance, contre un vrai PostgreSQL.
//!
//! Le balayage est un `UPDATE … RETURNING` avec `SKIP LOCKED` : ni le domaine
//! ni un double en mémoire ne diraient si deux passages concurrents se
//! partagent le travail au lieu de se marcher dessus, ni si la relance est
//! réellement idempotente.

use chrono::{Duration, Utc};
use klaar_application::ports::demande_repository::DemandeRepository;
use klaar_catalog::CodeCatalogue;
use klaar_identity::{EmpreinteMotDePasse, MotDePasse, ParametresArgon2};
use klaar_matching::{
    Demande, MotifAnnulation, StatutDemande, Urgence, DUREE_DIFFUSION_SECONDES, RAYONS_METRES,
};
use klaar_shared_kernel::{Email, Geo};
use klaar_sqlx_repos::demonstration::compte_actif_de_demonstration;
use klaar_sqlx_repos::{creer_pool, PgDemandeRepository, PoolPg};
use std::sync::Arc;
use uuid::Uuid;

/// Grand-Place.
const CENTRE: (f64, f64) = (50.8467, 4.3525);

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

async fn compte(pool: &PoolPg) -> Uuid {
    let email = Email::parse(&format!("diff-{}@example.eu", Uuid::new_v4())).unwrap();
    let empreinte = EmpreinteMotDePasse::calculer(
        &MotDePasse::parse("Marie@2026Secure").unwrap(),
        ParametresArgon2::tests(),
    )
    .unwrap();
    compte_actif_de_demonstration(pool, &email, &empreinte)
        .await
        .expect("compte de test")
}

/// Crée une Demande dont le tour a commencé il y a `age_secondes`.
async fn demande(depot: &PgDemandeRepository, pool: &PoolPg, age_secondes: i64) -> Demande {
    let mut d = Demande::soumettre(
        compte(pool).await,
        CodeCatalogue::parse("plomberie").unwrap(),
        "Fuite sous l'évier",
        Geo::new(CENTRE.0, CENTRE.1).unwrap(),
        Urgence::Haute,
        Utc::now() - Duration::seconds(age_secondes),
    )
    .expect("Demande valide");
    d.diffuse_depuis = d.cree_le;
    depot.creer(&d).await.expect("création");
    d
}

fn echeance() -> chrono::DateTime<Utc> {
    Utc::now() - Duration::seconds(DUREE_DIFFUSION_SECONDES)
}

#[tokio::test]
async fn happy_un_tour_ecoule_passe_en_no_match_et_est_rendu() {
    let pool = pool().await;
    let depot = PgDemandeRepository::new(pool.clone());
    let d = demande(&depot, &pool, 60).await;

    // La file est vidée plutôt que balayée une fois : un passage rend les plus
    // anciennes d'abord, et une base qui garde les Demandes des exécutions
    // précédentes évince facilement celle-ci du premier lot.
    loop {
        if depot
            .expirer_echues(echeance(), 500)
            .await
            .unwrap()
            .is_empty()
        {
            break;
        }
    }

    // **Ce qui est asserté est l'issue, pas qui l'a produite.** Un autre binaire
    // de test balaie la même table au même moment : si son passage éteint cette
    // Demande avant le nôtre, le nôtre ne la rendra pas — et ce serait le
    // comportement correct, puisqu'un balayage ne rend que ce qu'il vient
    // d'éteindre. Assertée telle quelle, la propriété « elle nous est rendue »
    // dépendait de ce qui tournait en parallèle.
    //
    // Que le balayage rende exactement ce qu'il éteint est vérifié là où c'est
    // isolable : `usecases::expirer`, sur un double en mémoire.
    let relue = depot.par_id(d.id).await.unwrap().expect("la Demande");
    assert_eq!(relue.statut, StatutDemande::SansReponse);
}

#[tokio::test]
async fn happy_une_demande_encore_dans_sa_fenetre_est_laissee_tranquille() {
    let pool = pool().await;
    let depot = PgDemandeRepository::new(pool.clone());
    let d = demande(&depot, &pool, 0).await;

    let eteintes = depot.expirer_echues(echeance(), 500).await.unwrap();
    assert!(!eteintes.iter().any(|e| e.id == d.id));
    assert_eq!(
        depot.par_id(d.id).await.unwrap().unwrap().statut,
        StatutDemande::Diffusion
    );
}

#[tokio::test]
async fn edge_un_second_passage_ne_rend_plus_la_meme_demande() {
    // C'est ce qui évite de réveiller deux fois quelqu'un pour la même
    // mauvaise nouvelle.
    let pool = pool().await;
    let depot = PgDemandeRepository::new(pool.clone());
    let d = demande(&depot, &pool, 60).await;

    depot.expirer_echues(echeance(), 500).await.unwrap();
    let second = depot.expirer_echues(echeance(), 500).await.unwrap();
    assert!(!second.iter().any(|e| e.id == d.id));
}

#[tokio::test]
async fn edge_deux_balayages_concurrents_ne_rendent_jamais_deux_fois_la_meme() {
    // La propriété qui compte : aucune Demande n'est rendue par les deux
    // passages, sinon son auteur serait notifié deux fois.
    //
    // L'assertion ne porte pas sur « mes » Demandes. La base garde celles des
    // exécutions précédentes, et le premier jet de ce test échouait pour cette
    // seule raison : les siennes, les plus récentes, tombaient hors de la
    // fenêtre du balayage. La non-duplication, elle, se vérifie sur tout ce qui
    // est rendu, quelle qu'en soit l'origine.
    let pool = pool().await;
    let depot = Arc::new(PgDemandeRepository::new(pool.clone()));
    let mut miennes = Vec::new();
    for _ in 0..6 {
        miennes.push(demande(&depot, &pool, 60).await.id);
    }

    let echeance = echeance();
    let (a, b) = tokio::join!(
        {
            let d = depot.clone();
            tokio::spawn(async move { d.expirer_echues(echeance, 500).await.unwrap() })
        },
        {
            let d = depot.clone();
            tokio::spawn(async move { d.expirer_echues(echeance, 500).await.unwrap() })
        },
    );
    let (a, b) = (a.unwrap(), b.unwrap());

    let mut vus = std::collections::HashSet::new();
    for d in a.iter().chain(b.iter()) {
        assert!(
            vus.insert(d.id),
            "la Demande {} a été rendue deux fois",
            d.id
        );
    }

    // Et en vidant la file, toutes les miennes finissent éteintes : le balayage
    // ne perd rien, il se contente de ne pas doubler.
    while !depot
        .expirer_echues(echeance, 500)
        .await
        .unwrap()
        .is_empty()
    {}
    for id in &miennes {
        assert_eq!(
            depot.par_id(*id).await.unwrap().unwrap().statut,
            StatutDemande::SansReponse,
            "Demande {id}"
        );
    }
}

#[tokio::test]
async fn edge_un_passage_ne_depasse_jamais_la_limite() {
    // Ce qui borne un rattrapage après une longue interruption. L'assertion
    // porte sur « jamais plus de deux », et non sur « exactement deux » : un
    // autre cas de cette suite peut vider la file entre-temps, ce qui a fait
    // échouer le premier jet de ce test sans qu'aucune borne ne soit violée.
    let pool = pool().await;
    let depot = PgDemandeRepository::new(pool.clone());
    for _ in 0..4 {
        demande(&depot, &pool, 60).await;
    }

    loop {
        let moisson = depot.expirer_echues(echeance(), 2).await.unwrap();
        assert!(moisson.len() <= 2, "moisson de {} lignes", moisson.len());
        if moisson.is_empty() {
            break;
        }
    }
}

#[tokio::test]
async fn happy_la_relance_ecrit_rayon_compteur_et_debut_de_tour() {
    let pool = pool().await;
    let depot = PgDemandeRepository::new(pool.clone());
    let mut d = demande(&depot, &pool, 60).await;

    let maintenant = Utc::now();
    d.expirer(maintenant);
    d.elargir(maintenant).unwrap();
    assert!(depot.relancer(&d).await.unwrap());

    let relue = depot.par_id(d.id).await.unwrap().expect("la Demande");
    assert_eq!(relue.statut, StatutDemande::Diffusion);
    assert_eq!(relue.rayon_metres, RAYONS_METRES[1]);
    assert_eq!(relue.elargissements, 1);
    assert_eq!(relue.diffuse_depuis.timestamp(), maintenant.timestamp());
}

#[tokio::test]
async fn security_deux_relances_du_meme_etat_n_en_appliquent_qu_une() {
    // Le compare-and-swap porte sur le compteur : rejouer la même relance
    // brûlerait deux des trois chances du demandeur.
    let pool = pool().await;
    let depot = PgDemandeRepository::new(pool.clone());
    let mut d = demande(&depot, &pool, 60).await;

    let maintenant = Utc::now();
    d.expirer(maintenant);
    d.elargir(maintenant).unwrap();
    assert!(depot.relancer(&d).await.unwrap());
    assert!(
        !depot.relancer(&d).await.unwrap(),
        "la seconde relance du même état doit être refusée"
    );
    assert_eq!(depot.par_id(d.id).await.unwrap().unwrap().elargissements, 1);
}

#[tokio::test]
async fn security_une_demande_attribuee_ne_repart_pas_en_diffusion() {
    let pool = pool().await;
    let depot = PgDemandeRepository::new(pool.clone());
    let mut d = demande(&depot, &pool, 60).await;
    sqlx::query("UPDATE demande SET statut = 'MATCHED' WHERE id = $1")
        .bind(d.id)
        .execute(&pool)
        .await
        .unwrap();

    let maintenant = Utc::now();
    d.statut = StatutDemande::SansReponse;
    d.elargir(maintenant).unwrap();
    assert!(!depot.relancer(&d).await.unwrap());
    assert_eq!(
        depot.par_id(d.id).await.unwrap().unwrap().statut,
        StatutDemande::Attribuee
    );
}

#[tokio::test]
async fn security_le_balayage_ne_touche_ni_les_attribuees_ni_les_annulees() {
    let pool = pool().await;
    let depot = PgDemandeRepository::new(pool.clone());
    let attribuee = demande(&depot, &pool, 300).await;
    let annulee = demande(&depot, &pool, 300).await;
    sqlx::query("UPDATE demande SET statut = 'MATCHED' WHERE id = $1")
        .bind(attribuee.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE demande SET statut = 'CANCELLED' WHERE id = $1")
        .bind(annulee.id)
        .execute(&pool)
        .await
        .unwrap();

    let eteintes = depot.expirer_echues(echeance(), 500).await.unwrap();
    assert!(!eteintes.iter().any(|d| d.id == attribuee.id));
    assert!(!eteintes.iter().any(|d| d.id == annulee.id));
    assert_eq!(
        depot.par_id(attribuee.id).await.unwrap().unwrap().statut,
        StatutDemande::Attribuee
    );
}

#[tokio::test]
async fn happy_l_annulation_part_de_diffusion_comme_de_sans_reponse() {
    let pool = pool().await;
    let depot = PgDemandeRepository::new(pool.clone());
    let diffusee = demande(&depot, &pool, 0).await;
    let sans_reponse = demande(&depot, &pool, 60).await;
    depot.expirer_echues(echeance(), 500).await.unwrap();

    for id in [diffusee.id, sans_reponse.id] {
        assert!(
            depot
                .annuler(id, Some(MotifAnnulation::TrouveAilleurs))
                .await
                .unwrap(),
            "Demande {id}"
        );
        let relue = depot.par_id(id).await.unwrap().unwrap();
        assert_eq!(relue.statut, StatutDemande::Annulee);
        // Le motif fait l'aller-retour : c'est ce que FR-014 veut conserver.
        assert_eq!(
            relue.motif_annulation,
            Some(MotifAnnulation::TrouveAilleurs)
        );
    }
}

#[tokio::test]
async fn security_une_demande_attribuee_ne_porte_jamais_de_motif() {
    // La contrainte de base le grave : sans elle, une Demande attribuée
    // pourrait porter le motif d'une annulation qui n'a pas eu lieu, et
    // l'analyse compterait des annulations imaginaires.
    let pool = pool().await;
    let depot = PgDemandeRepository::new(pool.clone());
    let d = demande(&depot, &pool, 0).await;
    sqlx::query("UPDATE demande SET statut = 'MATCHED' WHERE id = $1")
        .bind(d.id)
        .execute(&pool)
        .await
        .unwrap();

    let refus = sqlx::query("UPDATE demande SET motif_annulation = 'TOO_SLOW' WHERE id = $1")
        .bind(d.id)
        .execute(&pool)
        .await;
    assert!(
        refus.is_err(),
        "la base doit refuser un motif sur une Demande non annulée"
    );
}

#[tokio::test]
async fn security_une_demande_attribuee_ne_s_annule_pas() {
    // À ce stade, c'est la Mission qu'il faut annuler (FR-023) : effacer la
    // Demande laisserait un prestataire en route sans que rien ne le dise.
    let pool = pool().await;
    let depot = PgDemandeRepository::new(pool.clone());
    let d = demande(&depot, &pool, 0).await;
    sqlx::query("UPDATE demande SET statut = 'MATCHED' WHERE id = $1")
        .bind(d.id)
        .execute(&pool)
        .await
        .unwrap();

    assert!(!depot.annuler(d.id, None).await.unwrap());
    assert_eq!(
        depot.par_id(d.id).await.unwrap().unwrap().statut,
        StatutDemande::Attribuee
    );
}
