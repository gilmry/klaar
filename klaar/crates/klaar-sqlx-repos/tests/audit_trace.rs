//! Story 3.8 — trace immuable, chaînée et auditable, contre un vrai PostgreSQL.
//!
//! Trois choses ne se testent qu'ici : le déclencheur d'immuabilité, qui est du
//! PL/pgSQL ; la chaîne de signatures, dont la tête vit dans une table ; et
//! l'agrégat géographique, qui est du SQL spatial.

use chrono::{Duration, Utc};
use klaar_application::ports::demande_repository::DemandeRepository;
use klaar_application::ports::provider_repository::ProviderRepository;
use klaar_application::ports::trace_repository::{LigneTrace, MotifEcart, TraceRepository};
use klaar_audit_adapter::{contenu_canonique, SignataireTrace};
use klaar_catalog::CodeCatalogue;
use klaar_identity::{
    EmpreinteMotDePasse, MotDePasse, NumeroBce, ParametresArgon2, PreuveKyc, Provider,
};
use klaar_matching::{calculer_score, Demande, Urgence, RAYONS_METRES};
use klaar_shared_kernel::{Email, Geo};
use klaar_sqlx_repos::audit_trace::{disparite_geographique, verifier_chaine};
use klaar_sqlx_repos::demonstration::compte_actif_de_demonstration;
use klaar_sqlx_repos::{
    creer_pool, PgDemandeRepository, PgProviderRepository, PgTraceRepository, PoolPg,
};
use std::sync::Arc;
use uuid::Uuid;

/// Grand-Place.
const CENTRE: (f64, f64) = (50.8467, 4.3525);
const CLE: &[u8] = b"cle-de-test-trace-de-trente-deux";

async fn pool() -> PoolPg {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL requise : `make db-up && make migrate`, ou service postgres en CI");
    creer_pool(&url).await.expect("connexion PostgreSQL")
}

fn signataire() -> Arc<SignataireTrace> {
    Arc::new(SignataireTrace::new(CLE).expect("clé de test valide"))
}

async fn compte(pool: &PoolPg) -> Uuid {
    let email = Email::parse(&format!("audit-{}@example.eu", Uuid::new_v4())).unwrap();
    let empreinte = EmpreinteMotDePasse::calculer(
        &MotDePasse::parse("Marie@2026Secure").unwrap(),
        ParametresArgon2::tests(),
    )
    .unwrap();
    compte_actif_de_demonstration(pool, &email, &empreinte)
        .await
        .expect("compte de test")
}

fn numero() -> NumeroBce {
    let corps = (Uuid::new_v4().as_u128() as u64) % 20_000_000;
    NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).expect("numéro construit")
}

async fn prestataire(pool: &PoolPg) -> Provider {
    let mut p = Provider::inscrire(
        compte(pool).await,
        numero(),
        "Prestataire audit",
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
    p
}

async fn demande(pool: &PoolPg, lat: f64, lon: f64) -> Demande {
    let d = Demande::soumettre(
        compte(pool).await,
        CodeCatalogue::parse("plomberie").unwrap(),
        "Fuite sous l'évier",
        Geo::new(lat, lon).unwrap(),
        Urgence::Haute,
        Utc::now(),
    )
    .expect("Demande valide");
    PgDemandeRepository::new(pool.clone())
        .creer(&d)
        .await
        .expect("création");
    d
}

/// Un maillon relu : sa signature et celle qu'il déclare pour prédécesseur.
type Maillon = (Option<Vec<u8>>, Option<Vec<u8>>);

/// Identifiant de la prochaine ligne de trace à écrire.
///
/// Les cas vérifient la chaîne **depuis leur propre première ligne** et non
/// depuis l'origine : la table est partagée, et une seule ligne signée avec une
/// autre clé — une vérification manuelle, une rotation — casserait
/// définitivement le rejeu complet pour tous les cas. C'est arrivé, et c'est ce
/// qui a motivé le rejeu borné.
async fn prochaine_ligne(pool: &PoolPg) -> i64 {
    let max: Option<i64> = sqlx::query_scalar("SELECT MAX(id) FROM trace_matching")
        .fetch_one(pool)
        .await
        .expect("dernier identifiant");
    max.unwrap_or(0) + 1
}

fn ligne(demande_id: Uuid, provider_id: Uuid, retenu: bool) -> LigneTrace {
    LigneTrace {
        demande_id,
        provider_id,
        score: calculer_score(1_200.0, RAYONS_METRES[0], 30.0, None),
        distance_metres: 1_200.0,
        retenu,
        motif_ecart: (!retenu).then_some(MotifEcart::HorsTop),
        tracee_le: Utc::now(),
    }
}

#[tokio::test]
async fn happy_une_trace_signee_se_verifie() {
    let pool = pool().await;
    let depart = prochaine_ligne(&pool).await;
    let depot = PgTraceRepository::avec_signature(pool.clone(), signataire());
    let d = demande(&pool, CENTRE.0, CENTRE.1).await;
    let p = prestataire(&pool).await;

    depot
        .consigner(&[ligne(d.id, p.id, true)])
        .await
        .expect("trace écrite");

    let integrite = verifier_chaine(
        &pool,
        Some(&SignataireTrace::new(CLE).unwrap()),
        Some(depart),
    )
    .await
    .unwrap();
    assert_eq!(integrite.rompue_a, None, "la chaîne doit tenir");
    assert!(integrite.verifiees > 0);
    assert!(integrite.cle_disponible);
}

#[tokio::test]
async fn happy_la_chaine_relie_des_tours_successifs() {
    // Deux tours de matching distincts : chaque maillon doit déclarer pour
    // prédécesseur la ligne signée réellement écrite avant lui.
    let pool = pool().await;
    let depot = PgTraceRepository::avec_signature(pool.clone(), signataire());
    let p = prestataire(&pool).await;
    let a = demande(&pool, CENTRE.0, CENTRE.1).await;
    let b = demande(&pool, CENTRE.0, CENTRE.1).await;

    depot.consigner(&[ligne(a.id, p.id, true)]).await.unwrap();
    depot.consigner(&[ligne(b.id, p.id, false)]).await.unwrap();

    let sigs: Vec<Maillon> = sqlx::query_as(
        "SELECT signature, signature_precedente FROM trace_matching
         WHERE demande_id IN ($1, $2) ORDER BY id",
    )
    .bind(a.id)
    .bind(b.id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(sigs.len(), 2);
    // Les deux maillons sont scellés et déclarent un prédécesseur.
    for (i, (signature, precedente)) in sigs.iter().enumerate() {
        assert!(signature.is_some(), "maillon {i} non scellé");
        assert!(precedente.is_some(), "maillon {i} sans prédécesseur");
    }

    // **Le prédécesseur déclaré est celui réellement écrit avant.** L'adjacence
    // des deux tours n'est pas assertée : cette suite tourne en parallèle, et
    // un autre cas peut s'insérer entre eux. Ce qui est vérifié est plus
    // précis — chaque maillon désigne la ligne signée qui le précède
    // réellement dans la table, quelle qu'elle soit.
    //
    // **Pourquoi pas `verifier_chaine` ici.** Le rejeu complet porte sur une
    // fenêtre partagée avec les autres cas et avec les autres binaires de test,
    // qui écrivent dans la même table : son résultat dépend alors de ce qui a
    // tourné en même temps, et l'assertion devient intermittente. C'est
    // `happy_une_trace_signee_se_verifie` qui l'exerce, sur une fenêtre d'une
    // seule ligne où l'interférence est bien plus étroite.
    for (signature, precedente) in &sigs {
        let precedente_reelle: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT signature FROM trace_matching
             WHERE signature IS NOT NULL AND id < (
                 SELECT id FROM trace_matching WHERE signature = $1
             )
             ORDER BY id DESC LIMIT 1",
        )
        .bind(signature.as_deref())
        .fetch_optional(&pool)
        .await
        .unwrap()
        .flatten();
        assert_eq!(
            precedente.as_deref(),
            precedente_reelle.as_deref(),
            "un maillon déclare un prédécesseur qui n'est pas celui écrit avant lui"
        );
    }

    // Et les deux maillons sont bien scellés par **notre** clé : une signature
    // cohérente mais étrangère ne prouverait rien.
    let signataire = SignataireTrace::new(CLE).unwrap();
    for (demande_id, (signature, precedente)) in [a.id, b.id].iter().zip(sigs.iter()) {
        let contenu: (
            Uuid,
            f64,
            f64,
            bool,
            Option<String>,
            chrono::DateTime<chrono::Utc>,
        ) = sqlx::query_as(
            "SELECT provider_id, score, distance_metres, retenu, motif_ecart, tracee_le
                 FROM trace_matching WHERE demande_id = $1",
        )
        .bind(demande_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            signataire.verifier(
                precedente.as_deref(),
                &contenu_canonique(
                    demande_id,
                    &contenu.0,
                    contenu.1,
                    contenu.2,
                    contenu.3,
                    contenu.4.as_deref(),
                    contenu.5.timestamp(),
                ),
                signature.as_deref().unwrap(),
            ),
            "maillon non vérifiable par la clé de la trace"
        );
    }
}

#[tokio::test]
async fn security_la_trace_ne_se_modifie_pas() {
    // « On ne modifie pas cette table » est une phrase ; le déclencheur est un
    // refus.
    let pool = pool().await;
    let depot = PgTraceRepository::avec_signature(pool.clone(), signataire());
    let d = demande(&pool, CENTRE.0, CENTRE.1).await;
    let p = prestataire(&pool).await;
    depot.consigner(&[ligne(d.id, p.id, false)]).await.unwrap();

    let refus = sqlx::query("UPDATE trace_matching SET retenu = TRUE WHERE demande_id = $1")
        .bind(d.id)
        .execute(&pool)
        .await;
    assert!(refus.is_err(), "un UPDATE doit être refusé");
    assert!(format!("{:?}", refus.unwrap_err()).contains("append-only"));
}

#[tokio::test]
async fn security_la_trace_ne_se_supprime_pas() {
    let pool = pool().await;
    let depot = PgTraceRepository::avec_signature(pool.clone(), signataire());
    let d = demande(&pool, CENTRE.0, CENTRE.1).await;
    let p = prestataire(&pool).await;
    depot.consigner(&[ligne(d.id, p.id, true)]).await.unwrap();

    let refus = sqlx::query("DELETE FROM trace_matching WHERE demande_id = $1")
        .bind(d.id)
        .execute(&pool)
        .await;
    assert!(refus.is_err(), "un DELETE doit être refusé");
}

#[tokio::test]
async fn security_supprimer_une_demande_echoue_bruyamment() {
    // La cascade emporterait la trace en silence. Le déclencheur transforme
    // cela en échec visible, ce qui est la bonne réponse : la trace relève
    // d'une obligation légale (AI Act art. 12, RGPD art. 17 §3 b)).
    let pool = pool().await;
    let depot = PgTraceRepository::avec_signature(pool.clone(), signataire());
    let d = demande(&pool, CENTRE.0, CENTRE.1).await;
    let p = prestataire(&pool).await;
    depot.consigner(&[ligne(d.id, p.id, true)]).await.unwrap();

    let refus = sqlx::query("DELETE FROM demande WHERE id = $1")
        .bind(d.id)
        .execute(&pool)
        .await;
    assert!(
        refus.is_err(),
        "supprimer une Demande tracée doit échouer, pas emporter la trace"
    );
}

#[tokio::test]
async fn security_une_cle_differente_ne_verifie_rien() {
    let pool = pool().await;
    let depart = prochaine_ligne(&pool).await;
    let depot = PgTraceRepository::avec_signature(pool.clone(), signataire());
    let d = demande(&pool, CENTRE.0, CENTRE.1).await;
    let p = prestataire(&pool).await;
    depot.consigner(&[ligne(d.id, p.id, true)]).await.unwrap();

    let autre = SignataireTrace::new(b"une-AUTRE-cle-de-trente-deux-oct").unwrap();
    let integrite = verifier_chaine(&pool, Some(&autre), Some(depart))
        .await
        .unwrap();
    assert!(
        integrite.rompue_a.is_some(),
        "une clé étrangère ne doit rien pouvoir valider"
    );
}

#[tokio::test]
async fn security_sans_cle_rien_n_est_declare_verifie() {
    // Un rapport rassurant sans preuve serait le pire des résultats.
    let pool = pool().await;
    let depart = prochaine_ligne(&pool).await;
    let depot = PgTraceRepository::avec_signature(pool.clone(), signataire());
    let d = demande(&pool, CENTRE.0, CENTRE.1).await;
    let p = prestataire(&pool).await;
    depot.consigner(&[ligne(d.id, p.id, true)]).await.unwrap();

    let integrite = verifier_chaine(&pool, None, Some(depart)).await.unwrap();
    assert_eq!(integrite.verifiees, 0);
    assert!(!integrite.cle_disponible);
    assert!(integrite.non_signees > 0);
}

#[tokio::test]
async fn edge_une_trace_non_signee_reste_ecrite_et_comptee_a_part() {
    // Un déploiement sans clé écrit une trace non signée plutôt que pas de
    // trace : celle-ci explique toujours une décision, l'absence non.
    let pool = pool().await;
    let depart = prochaine_ligne(&pool).await;
    let depot = PgTraceRepository::new(pool.clone());
    let d = demande(&pool, CENTRE.0, CENTRE.1).await;
    let p = prestataire(&pool).await;
    depot.consigner(&[ligne(d.id, p.id, true)]).await.unwrap();

    let ecrite: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM trace_matching WHERE demande_id = $1 AND signature IS NULL",
    )
    .bind(d.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ecrite, 1);

    let integrite = verifier_chaine(
        &pool,
        Some(&SignataireTrace::new(CLE).unwrap()),
        Some(depart),
    )
    .await
    .unwrap();
    assert!(integrite.non_signees > 0);
}

#[tokio::test]
async fn edge_un_second_tour_sur_la_meme_demande_ne_casse_pas_la_chaine() {
    // `ON CONFLICT DO NOTHING` : la ligne n'est pas réécrite, donc la tête de
    // chaîne ne doit pas avancer. Sinon la vérification échouerait sur une
    // trace pourtant intacte.
    let pool = pool().await;
    let depart = prochaine_ligne(&pool).await;
    let depot = PgTraceRepository::avec_signature(pool.clone(), signataire());
    let d = demande(&pool, CENTRE.0, CENTRE.1).await;
    let p = prestataire(&pool).await;

    depot.consigner(&[ligne(d.id, p.id, true)]).await.unwrap();
    depot.consigner(&[ligne(d.id, p.id, false)]).await.unwrap();

    assert_eq!(
        verifier_chaine(
            &pool,
            Some(&SignataireTrace::new(CLE).unwrap()),
            Some(depart)
        )
        .await
        .unwrap()
        .rompue_a,
        None,
        "un doublon écarté ne doit pas rompre la chaîne"
    );
}

#[tokio::test]
async fn happy_l_agregat_geographique_compte_les_demandes_par_maille() {
    let pool = pool().await;
    // Cinq Demandes au même endroit : de quoi passer le seuil de k-anonymat.
    for _ in 0..5 {
        demande(&pool, CENTRE.0, CENTRE.1).await;
    }

    let rapport = disparite_geographique(&pool, Utc::now() - Duration::days(1), 0.01, 5)
        .await
        .unwrap();
    assert!(rapport.demandes_totales >= 5);
    assert!(!rapport.mailles.is_empty());
    assert!(rapport
        .mailles
        .iter()
        .any(|m| m.maille.starts_with("50.84")));
}

#[tokio::test]
async fn security_une_maille_sous_le_seuil_est_supprimee_et_annoncee() {
    // Une maille où deux Demandes ont été émises désignerait des personnes.
    // Les taire ferait passer une couverture partielle pour complète.
    //
    // Le seuil est poussé très haut plutôt que de compter sur un coin de la
    // Région où rien n'aurait été soumis : la base garde les Demandes des
    // exécutions précédentes, et le premier jet de ce cas a fini par échouer
    // quand son coin « isolé » a franchi le seuil à force d'être rejoué. Ce
    // qu'on vérifie ici ne dépend d'aucun état préalable.
    let pool = pool().await;
    demande(&pool, 50.7700, 4.4400).await;

    let rapport = disparite_geographique(&pool, Utc::now() - Duration::days(1), 0.01, 1_000_000)
        .await
        .unwrap();
    assert!(
        rapport.mailles.is_empty(),
        "toutes les mailles sont sous le seuil"
    );
    assert!(rapport.mailles_supprimees_sous_le_seuil > 0);
    // Elles comptent quand même dans le total : le rapport ne prétend pas que
    // ces Demandes n'existent pas, seulement qu'il ne peut pas les situer.
    assert!(rapport.demandes_totales > 0);
    // Et l'écart n'est pas inventé sur un ensemble vide.
    assert_eq!(rapport.ecart_de_taux_d_attribution, None);
}

#[tokio::test]
async fn edge_une_periode_vide_ne_produit_pas_d_ecart_invente() {
    // Sans Demande, `max - min` sur un ensemble vide donnerait `-inf` : le
    // rapport doit dire « rien à comparer », pas produire un nombre absurde.
    let pool = pool().await;
    let rapport = disparite_geographique(&pool, Utc::now() + Duration::days(1), 0.01, 5)
        .await
        .unwrap();
    assert_eq!(rapport.mailles_retenues, 0);
    assert_eq!(rapport.ecart_de_taux_d_attribution, None);
}
