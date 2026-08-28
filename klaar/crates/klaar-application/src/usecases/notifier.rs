//! Cas d'usage « notifier les candidats » (Story 3.3, FR-012).
//!
//! **Ce qu'une notification a le droit de dire.** Elle s'affiche sur un écran
//! verrouillé, lisible par quiconque passe à côté du téléphone. Elle ne porte
//! donc **ni la description du problème, ni l'adresse, ni rien du demandeur** :
//! seulement le secteur, la distance arrondie et l'urgence. C'est assez pour
//! décider d'ouvrir l'application, et c'est tout ce qu'il faut.
//!
//! La charge est chiffrée en transit (RFC 8291), mais le chiffrement ne protège
//! pas de ce que l'écran affiche. Les deux problèmes sont distincts, et seul le
//! second se règle en choisissant ce qu'on écrit.
//!
//! **Un abonnement mort est supprimé, pas réessayé.** Quand le service de push
//! répond 410, l'appareil a désinstallé ou révoqué : garder la ligne, c'est
//! conserver une donnée personnelle devenue sans finalité, et réessayer sans
//! fin.

use klaar_matching::{Demande, Urgence};
use klaar_shared_kernel::Locale;
use uuid::Uuid;

use crate::ports::erreurs::RepositoryError;
use crate::ports::push::{PushError, PushMessage, PushNotifier};
use crate::ports::push_repository::PushSubscriptionRepository;
use crate::usecases::matcher::Candidat;

/// Résultat d'un tour de notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BilanNotification {
    /// Appareils qui ont reçu le message.
    pub notifies: usize,
    /// Candidats sans aucun abonnement : ils verront la Demande en ouvrant
    /// l'application, ce qui n'est pas un échec.
    pub sans_abonnement: usize,
    /// Abonnements supprimés parce que le service de push les a déclarés
    /// disparus.
    pub purges: usize,
}

/// Arrondit la distance pour l'affichage.
///
/// À la centaine de mètres sous le kilomètre, au dixième de kilomètre au-delà.
/// Une distance au mètre près sur un écran verrouillé situerait le demandeur
/// bien plus précisément que nécessaire, et la précision n'apporte rien à qui
/// décide s'il peut y aller.
fn distance_lisible(metres: f64) -> String {
    if metres < 1_000.0 {
        format!("{} m", ((metres / 100.0).round() * 100.0) as i64)
    } else {
        format!("{:.1} km", metres / 1_000.0)
    }
}

fn libelle_urgence(urgence: Urgence, locale: Locale) -> &'static str {
    match (urgence, locale) {
        (Urgence::Haute, Locale::Fr) => "tout de suite",
        (Urgence::Haute, Locale::Nl) => "onmiddellijk",
        (Urgence::Haute, Locale::En) => "right away",
        (Urgence::Normale, Locale::Fr) => "dans la journée",
        (Urgence::Normale, Locale::Nl) => "vandaag",
        (Urgence::Normale, Locale::En) => "today",
        (Urgence::Basse, Locale::Fr) => "peut attendre",
        (Urgence::Basse, Locale::Nl) => "kan wachten",
        (Urgence::Basse, Locale::En) => "can wait",
    }
}

/// Compose le message affiché.
///
/// Le secteur est donné par son **code**, pas par son libellé : le cas d'usage
/// ne connaît pas le catalogue, et un code lisible comme `plomberie` reste
/// compréhensible. Le libellé traduit viendra quand la notification passera par
/// une vue qui a le catalogue sous la main.
pub fn composer(demande: &Demande, distance_metres: f64, locale: Locale) -> PushMessage {
    let titre = match locale {
        Locale::Fr => format!("Demande — {}", demande.secteur),
        Locale::Nl => format!("Aanvraag — {}", demande.secteur),
        Locale::En => format!("Request — {}", demande.secteur),
    };
    PushMessage {
        titre,
        corps: format!(
            "{} · {}",
            distance_lisible(distance_metres),
            libelle_urgence(demande.urgence, locale)
        ),
        url: format!("/prestataire/demande?id={}", demande.id),
        // Un `tag` par Demande : deux notifications pour la même Demande se
        // remplacent au lieu de s'empiler.
        tag: Some(format!("demande-{}", demande.id)),
    }
}

/// Compose l'avis « quelqu'un d'autre a pris cette Demande » (FR-013).
///
/// Même discrétion que le message d'origine, et pour la même raison : il
/// s'affiche au même endroit. Il ne dit pas non plus **qui** l'a prise — cela
/// désignerait un concurrent nommément à quatre autres entreprises, ce que
/// personne n'a demandé.
///
/// Le `tag` est celui de la Demande : cet avis **remplace** la notification
/// d'origine au lieu de s'ajouter à elle. Laisser les deux côte à côte ferait
/// partir quelqu'un pour une intervention déjà attribuée.
pub fn composer_match_pris(demande: &Demande, locale: Locale) -> PushMessage {
    let (titre, corps) = match locale {
        Locale::Fr => ("Demande déjà prise", "Un autre prestataire a répondu"),
        Locale::Nl => (
            "Aanvraag al toegewezen",
            "Een andere vakman heeft geantwoord",
        ),
        Locale::En => ("Request already taken", "Another provider responded"),
    };
    PushMessage {
        titre: format!("{titre} — {}", demande.secteur),
        corps: corps.to_string(),
        url: "/prestataire".to_string(),
        tag: Some(format!("demande-{}", demande.id)),
    }
}

/// Compose l'avis « personne n'a répondu » adressé au demandeur (FR-015).
///
/// Il s'affiche sur **son** écran verrouillé à lui : il peut donc nommer le
/// secteur, mais toujours pas la description ni l'adresse — un écran verrouillé
/// reste lisible par qui passe à côté, y compris chez soi.
///
/// Le message dit ce qu'il reste à faire. Un « personne n'a répondu » sans
/// suite laisserait quelqu'un devant une fuite sans savoir s'il doit attendre
/// ou appeler ailleurs.
pub fn composer_sans_reponse(demande: &Demande, locale: Locale) -> PushMessage {
    let (titre, corps) = match locale {
        Locale::Fr => (
            "Personne n'a répondu",
            "Élargissez la zone de recherche ou annulez",
        ),
        Locale::Nl => (
            "Niemand heeft geantwoord",
            "Vergroot de zoekzone of annuleer",
        ),
        Locale::En => ("Nobody responded", "Widen the search area or cancel"),
    };
    PushMessage {
        titre: format!("{titre} — {}", demande.secteur),
        corps: corps.to_string(),
        url: format!("/demande?id={}", demande.id),
        tag: Some(format!("demande-{}", demande.id)),
    }
}

/// Prévient le demandeur que son tour s'est terminé sans réponse (FR-015).
pub async fn notifier_sans_reponse<A, N>(
    abonnements: &A,
    notifieur: &N,
    demande: &Demande,
    locale: Locale,
) -> Result<BilanNotification, RepositoryError>
where
    A: PushSubscriptionRepository,
    N: PushNotifier,
{
    envoyer_a(
        abonnements,
        notifieur,
        &[demande.demandeur_id],
        &composer_sans_reponse(demande, locale),
    )
    .await
}

/// Compose l'avis « le demandeur a retiré sa Demande » (FR-014 `@happy`).
///
/// Ne dit pas **pourquoi**. Le motif appartient au demandeur ; le porter aux
/// prestataires notifiés en ferait un jugement diffusé à dix entreprises, et
/// « trouvé ailleurs » se lit vite comme un reproche.
pub fn composer_annulation(demande: &Demande, locale: Locale) -> PushMessage {
    let (titre, corps) = match locale {
        Locale::Fr => ("Demande annulée", "Le demandeur l'a retirée"),
        Locale::Nl => ("Aanvraag geannuleerd", "De aanvrager heeft ze ingetrokken"),
        Locale::En => ("Request cancelled", "The requester withdrew it"),
    };
    PushMessage {
        titre: format!("{titre} — {}", demande.secteur),
        corps: corps.to_string(),
        url: "/prestataire".to_string(),
        // Même `tag` : cet avis remplace la notification d'origine au lieu de
        // s'ajouter à elle, sinon quelqu'un part pour une Demande retirée.
        tag: Some(format!("demande-{}", demande.id)),
    }
}

/// Prévient les candidats notifiés que la Demande a été retirée (FR-014).
pub async fn notifier_annulation<A, N>(
    abonnements: &A,
    notifieur: &N,
    demande: &Demande,
    comptes: &[Uuid],
    locale: Locale,
) -> Result<BilanNotification, RepositoryError>
where
    A: PushSubscriptionRepository,
    N: PushNotifier,
{
    envoyer_a(
        abonnements,
        notifieur,
        comptes,
        &composer_annulation(demande, locale),
    )
    .await
}

/// Compose l'avis d'avancement d'une Mission, pour le demandeur (FR-018).
///
/// Il s'affiche sur **son** écran verrouillé. Il ne nomme pas le prestataire :
/// « Untel est en route » lu par-dessus une épaule dit qui vient chez qui. Le
/// secteur suffit à savoir de quelle intervention on parle.
pub fn composer_avancement(
    demande: &Demande,
    statut: klaar_intervention::StatutMission,
    locale: Locale,
) -> PushMessage {
    use klaar_intervention::StatutMission as S;
    let corps = match (statut, locale) {
        (S::EnRoute, Locale::Fr) => "Le prestataire est en route",
        (S::EnRoute, Locale::Nl) => "De vakman is onderweg",
        (S::EnRoute, Locale::En) => "The provider is on the way",
        (S::SurPlace, Locale::Fr) => "Le prestataire est arrivé",
        (S::SurPlace, Locale::Nl) => "De vakman is ter plaatse",
        (S::SurPlace, Locale::En) => "The provider has arrived",
        (S::Terminee, Locale::Fr) => "L'intervention est terminée",
        (S::Terminee, Locale::Nl) => "De interventie is afgerond",
        (S::Terminee, Locale::En) => "The job is done",
        (S::Annulee, Locale::Fr) => "L'intervention a été annulée",
        (S::Annulee, Locale::Nl) => "De interventie is geannuleerd",
        (S::Annulee, Locale::En) => "The job was cancelled",
        // Surtout utile quand c'est le délai de soixante-douze heures qui a
        // validé à sa place : il doit l'apprendre, y compris — et surtout — s'il
        // n'a pas rouvert l'application depuis trois jours.
        (S::Validee, Locale::Fr) => "L'intervention a été validée",
        (S::Validee, Locale::Nl) => "De interventie is bevestigd",
        (S::Validee, Locale::En) => "The job was validated",
        // `ACCEPTED` n'est pas une transition : c'est l'état de naissance, et
        // le demandeur l'a déjà appris par la réponse à sa Demande.
        (S::Acceptee, Locale::Fr) => "Votre demande a été acceptée",
        (S::Acceptee, Locale::Nl) => "Uw aanvraag is aanvaard",
        (S::Acceptee, Locale::En) => "Your request was accepted",
    };
    PushMessage {
        titre: format!("{}", demande.secteur),
        corps: corps.to_string(),
        // Une étiquette par Mission et non par statut : les avis successifs se
        // remplacent, sinon un téléphone posé sur une table affiche trois
        // notifications pour une seule intervention.
        tag: Some(format!("mission-{}", demande.id)),
        url: format!("/demande?id={}", demande.id),
    }
}

/// Compose l'avis « vous avez reçu un devis », pour le demandeur (FR-016).
///
/// **Sans le montant.** Il s'affiche sur un écran verrouillé, que n'importe qui
/// peut lire par-dessus une épaule dans un tram. Ce que coûte la réparation
/// d'une chaudière est une information sur la situation de quelqu'un ; elle
/// mérite d'être ouverte, pas annoncée. Le prix est dans l'application, derrière
/// la session.
pub fn composer_devis_recu(demande: &Demande, locale: Locale) -> PushMessage {
    let corps = match locale {
        Locale::Fr => "Vous avez reçu un devis. Ouvrez pour le consulter.",
        Locale::Nl => "U hebt een offerte ontvangen. Open om ze te bekijken.",
        Locale::En => "You received a quote. Open to view it.",
    };
    PushMessage {
        titre: format!("{}", demande.secteur),
        corps: corps.to_string(),
        // Même étiquette que les avis d'avancement : un devis reçu est une
        // étape de la même intervention, et l'empiler à part afficherait deux
        // notifications pour une seule affaire.
        tag: Some(format!("mission-{}", demande.id)),
        url: format!("/demande?id={}", demande.id),
    }
}

/// Prévient le demandeur qu'un devis l'attend (FR-016 `@happy`).
pub async fn notifier_devis_recu<A, N>(
    abonnements: &A,
    notifieur: &N,
    demande: &Demande,
    locale: Locale,
) -> Result<BilanNotification, RepositoryError>
where
    A: PushSubscriptionRepository,
    N: PushNotifier,
{
    envoyer_a(
        abonnements,
        notifieur,
        &[demande.demandeur_id],
        &composer_devis_recu(demande, locale),
    )
    .await
}

/// Compose l'avis de réponse à un devis, pour le prestataire (FR-017).
///
/// **Sans le montant ni le motif.** Le prestataire connaît son prix ; ce qu'il
/// attend est un oui ou un non. Le motif du refus, lui, reste derrière la
/// session : « trop cher » sur un écran verrouillé, lu par-dessus une épaule,
/// n'apporte rien et expose ce que quelqu'un a pensé de son devis.
pub fn composer_reponse_devis(statut: klaar_payment::StatutDevis, locale: Locale) -> PushMessage {
    use klaar_payment::StatutDevis as S;
    let corps = match (statut, locale) {
        (S::Accepte, Locale::Fr) => "Votre devis a été accepté.",
        (S::Accepte, Locale::Nl) => "Uw offerte is aanvaard.",
        (S::Accepte, Locale::En) => "Your quote was accepted.",
        (S::Refuse, Locale::Fr) => "Votre devis a été refusé.",
        (S::Refuse, Locale::Nl) => "Uw offerte is geweigerd.",
        (S::Refuse, Locale::En) => "Your quote was declined.",
        // Injoignable : seules les deux réponses du demandeur mènent ici. Le
        // `match` reste exhaustif pour qu'un statut ajouté passe par là.
        (_, Locale::Fr) => "Votre devis a changé d'état.",
        (_, Locale::Nl) => "Uw offerte is van status veranderd.",
        (_, Locale::En) => "Your quote changed state.",
    };
    PushMessage {
        titre: "Klaar".to_string(),
        corps: corps.to_string(),
        tag: Some("devis-reponse".to_string()),
        url: "/prestataire".to_string(),
    }
}

/// Prévient le prestataire que son devis a reçu une réponse (FR-017).
pub async fn notifier_reponse_devis<A, N>(
    abonnements: &A,
    notifieur: &N,
    compte_prestataire: Uuid,
    statut: klaar_payment::StatutDevis,
    locale: Locale,
) -> Result<BilanNotification, RepositoryError>
where
    A: PushSubscriptionRepository,
    N: PushNotifier,
{
    envoyer_a(
        abonnements,
        notifieur,
        &[compte_prestataire],
        &composer_reponse_devis(statut, locale),
    )
    .await
}

/// Compose l'avis de validation, pour le prestataire (FR-021 `@happy`).
///
/// **Sans le montant.** Le prestataire connaît son prix ; ce qu'il attend est de
/// savoir que l'intervention est validée. Le distinguo entre une libération
/// autorisée et une libération en attente d'un second regard est dit, parce
/// qu'il change ce qu'il peut espérer et quand.
pub fn composer_liberation(statut: klaar_payment::StatutLiberation, locale: Locale) -> PushMessage {
    use klaar_payment::StatutLiberation as S;
    let corps = match (statut, locale) {
        (S::Autorisee, Locale::Fr) => "Votre intervention a été validée.",
        (S::Autorisee, Locale::Nl) => "Uw interventie is bevestigd.",
        (S::Autorisee, Locale::En) => "Your job was validated.",
        (S::EnAttenteOps, Locale::Fr) => "Intervention validée, versement en cours de contrôle.",
        (S::EnAttenteOps, Locale::Nl) => "Interventie bevestigd, uitbetaling in controle.",
        (S::EnAttenteOps, Locale::En) => "Job validated, payout under review.",
    };
    PushMessage {
        titre: "Klaar".to_string(),
        corps: corps.to_string(),
        tag: Some("liberation".to_string()),
        url: "/prestataire".to_string(),
    }
}

/// Prévient le prestataire que son intervention est validée (FR-021).
pub async fn notifier_liberation<A, N>(
    abonnements: &A,
    notifieur: &N,
    compte_prestataire: Uuid,
    statut: klaar_payment::StatutLiberation,
    locale: Locale,
) -> Result<BilanNotification, RepositoryError>
where
    A: PushSubscriptionRepository,
    N: PushNotifier,
{
    envoyer_a(
        abonnements,
        notifieur,
        &[compte_prestataire],
        &composer_liberation(statut, locale),
    )
    .await
}

/// Compose l'avis d'annulation d'une Mission, pour l'autre partie (FR-022).
///
/// **Sans le motif ni le montant.** L'écran verrouillé dit qu'il s'est passé
/// quelque chose ; ce que l'autre a coché comme raison est une opinion, et elle
/// n'a pas à se lire par-dessus une épaule.
pub fn composer_annulation_mission(
    auteur: klaar_intervention::AuteurAnnulation,
    locale: Locale,
) -> PushMessage {
    use klaar_intervention::AuteurAnnulation as A;
    let corps = match (auteur, locale) {
        (A::Demandeur, Locale::Fr) => "Le demandeur a annulé l'intervention.",
        (A::Demandeur, Locale::Nl) => "De aanvrager heeft de interventie geannuleerd.",
        (A::Demandeur, Locale::En) => "The requester cancelled the job.",
        (A::Prestataire, Locale::Fr) => "Le prestataire ne peut plus venir.",
        (A::Prestataire, Locale::Nl) => "De vakman kan niet meer komen.",
        (A::Prestataire, Locale::En) => "The provider can no longer come.",
    };
    PushMessage {
        titre: "Klaar".to_string(),
        corps: corps.to_string(),
        tag: Some("annulation-mission".to_string()),
        url: "/".to_string(),
    }
}

/// Prévient l'autre partie qu'une Mission a été annulée (FR-022 `@happy`).
pub async fn notifier_annulation_mission<A, N>(
    abonnements: &A,
    notifieur: &N,
    compte: Uuid,
    auteur: klaar_intervention::AuteurAnnulation,
    locale: Locale,
) -> Result<BilanNotification, RepositoryError>
where
    A: PushSubscriptionRepository,
    N: PushNotifier,
{
    envoyer_a(
        abonnements,
        notifieur,
        &[compte],
        &composer_annulation_mission(auteur, locale),
    )
    .await
}

/// Compose l'avis « nouveau message », pour l'autre partie (FR-030).
///
/// **Sans le contenu du message.** Il s'affiche sur un écran verrouillé : ce
/// que deux personnes se disent sur une intervention chez elles n'a pas à se
/// lire par-dessus une épaule. Le fil est dans l'application.
pub fn composer_message(locale: Locale) -> PushMessage {
    let corps = match locale {
        Locale::Fr => "Nouveau message sur votre intervention.",
        Locale::Nl => "Nieuw bericht over uw interventie.",
        Locale::En => "New message about your job.",
    };
    PushMessage {
        titre: "Klaar".to_string(),
        corps: corps.to_string(),
        // Une étiquette unique pour la conversation : dix messages ne doivent
        // pas empiler dix notifications.
        tag: Some("conversation".to_string()),
        url: "/".to_string(),
    }
}

/// Prévient l'autre partie qu'un message l'attend (FR-030 `@happy`).
pub async fn notifier_message<A, N>(
    abonnements: &A,
    notifieur: &N,
    destinataire: Uuid,
    locale: Locale,
) -> Result<BilanNotification, RepositoryError>
where
    A: PushSubscriptionRepository,
    N: PushNotifier,
{
    envoyer_a(
        abonnements,
        notifieur,
        &[destinataire],
        &composer_message(locale),
    )
    .await
}

/// Prévient le demandeur que sa Mission a avancé (FR-018 `@happy`).
pub async fn notifier_avancement<A, N>(
    abonnements: &A,
    notifieur: &N,
    demande: &Demande,
    statut: klaar_intervention::StatutMission,
    locale: Locale,
) -> Result<BilanNotification, RepositoryError>
where
    A: PushSubscriptionRepository,
    N: PushNotifier,
{
    envoyer_a(
        abonnements,
        notifieur,
        &[demande.demandeur_id],
        &composer_avancement(demande, statut, locale),
    )
    .await
}

/// Prévient les candidats qu'un autre a pris la Demande (FR-013 `@happy`).
///
/// Séparé de l'attribution elle-même : une panne du service de push ne doit pas
/// défaire une Mission déjà attribuée. Le prestataire retenu, lui, a sa réponse
/// HTTP et n'a rien à recevoir ici.
pub async fn notifier_match_pris<A, N>(
    abonnements: &A,
    notifieur: &N,
    demande: &Demande,
    comptes: &[Uuid],
    locale: Locale,
) -> Result<BilanNotification, RepositoryError>
where
    A: PushSubscriptionRepository,
    N: PushNotifier,
{
    envoyer_a(
        abonnements,
        notifieur,
        comptes,
        &composer_match_pris(demande, locale),
    )
    .await
}

/// Envoie un même message à une liste de comptes.
///
/// Le corps commun de tous les tours de notification : un abonnement déclaré
/// disparu est supprimé, une panne de transport n'interrompt pas le tour. Le
/// factoriser évite qu'un de ces deux comportements se perde dans une copie.
async fn envoyer_a<A, N>(
    abonnements: &A,
    notifieur: &N,
    comptes: &[Uuid],
    message: &PushMessage,
) -> Result<BilanNotification, RepositoryError>
where
    A: PushSubscriptionRepository,
    N: PushNotifier,
{
    let mut bilan = BilanNotification::default();

    for compte in comptes {
        let appareils = abonnements.lister_par_sujet(*compte).await?;
        if appareils.is_empty() {
            bilan.sans_abonnement += 1;
            continue;
        }
        for appareil in appareils {
            match notifieur.envoyer(&appareil.abonnement, message).await {
                Ok(()) => bilan.notifies += 1,
                Err(PushError::AbonnementExpire) => {
                    // 410 : garder la ligne conserverait une donnée personnelle
                    // sans finalité, et ferait réessayer sans fin.
                    abonnements
                        .supprimer_par_endpoint(&appareil.abonnement.endpoint)
                        .await?;
                    bilan.purges += 1;
                }
                Err(e) => {
                    // Les autres destinataires n'y sont pour rien.
                    tracing::warn!(erreur = %e, "notification non délivrée");
                }
            }
        }
    }

    Ok(bilan)
}

/// Prévient les candidats retenus, **chacun dans sa langue** (FR-012, FR-043).
///
/// **Le message est composé par destinataire et non une fois pour tous.** Un
/// tour de matching réveille jusqu'à dix prestataires, et rien ne dit qu'ils
/// parlent la même langue : à Bruxelles, c'est même le contraire qu'il faut
/// supposer. Composer une fois coûtait une allocation de moins et envoyait du
/// français à un néerlandophone.
pub async fn notifier<A, N, L>(
    abonnements: &A,
    notifieur: &N,
    langues: &L,
    demande: &Demande,
    candidats: &[Candidat],
) -> Result<BilanNotification, RepositoryError>
where
    A: PushSubscriptionRepository,
    N: PushNotifier,
    L: crate::ports::langue::LecteurLangue,
{
    let mut bilan = BilanNotification::default();

    for candidat in candidats {
        let appareils = abonnements
            .lister_par_sujet(candidat.utilisateur_id)
            .await?;
        if appareils.is_empty() {
            bilan.sans_abonnement += 1;
            continue;
        }

        // La langue est lue **après** avoir constaté qu'il y a un appareil à
        // joindre : sans abonnement, la lecture serait une requête pour rien.
        let locale = crate::usecases::langue::langue_de(langues, candidat.utilisateur_id).await;
        let message = composer(demande, candidat.distance_metres, locale);
        for appareil in appareils {
            match notifieur.envoyer(&appareil.abonnement, &message).await {
                Ok(()) => bilan.notifies += 1,
                Err(PushError::AbonnementExpire) => {
                    // 410 : l'appareil a désinstallé ou révoqué. Garder la
                    // ligne conserverait une donnée personnelle sans finalité,
                    // et ferait réessayer sans fin.
                    abonnements
                        .supprimer_par_endpoint(&appareil.abonnement.endpoint)
                        .await?;
                    bilan.purges += 1;
                }
                Err(e) => {
                    // Une panne du service de push ne doit pas interrompre le
                    // tour : les autres candidats n'y sont pour rien, et la
                    // Demande reste diffusée.
                    tracing::warn!(erreur = %e, "notification non délivrée");
                }
            }
        }
    }

    Ok(bilan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::push::PushSubscription;
    use crate::ports::push_repository::AbonnementEnregistre;
    use chrono::{DateTime, TimeZone, Utc};
    use klaar_catalog::CodeCatalogue;
    use klaar_matching::Score;
    use klaar_shared_kernel::Geo;
    use std::cell::RefCell;
    use uuid::Uuid;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn demande(urgence: Urgence) -> Demande {
        Demande::soumettre(
            Uuid::new_v4(),
            CodeCatalogue::parse("plomberie").unwrap(),
            "Fuite très reconnaissable sous l'évier de la cuisine",
            Geo::new(50.8467, 4.3525).unwrap(),
            urgence,
            instant(),
        )
        .unwrap()
    }

    fn candidat(utilisateur_id: Uuid, distance: f64) -> Candidat {
        Candidat {
            provider_id: Uuid::new_v4(),
            utilisateur_id,
            distance_metres: distance,
            score: klaar_matching::calculer_score(
                distance,
                klaar_matching::RAYONS_METRES[0],
                0.0,
                None,
            ),
        }
    }

    fn abonnement(endpoint: &str) -> PushSubscription {
        PushSubscription {
            endpoint: endpoint.to_string(),
            p256dh: "p".to_string(),
            auth: "a".to_string(),
        }
    }

    #[derive(Default)]
    struct AbonnementsMemoire {
        par_sujet: RefCell<Vec<(Uuid, PushSubscription)>>,
        supprimes: RefCell<Vec<String>>,
    }

    impl PushSubscriptionRepository for AbonnementsMemoire {
        async fn enregistrer(
            &self,
            _: &PushSubscription,
            _: Option<Uuid>,
        ) -> Result<AbonnementEnregistre, RepositoryError> {
            unreachable!()
        }

        async fn lister_par_sujet(
            &self,
            sujet_id: Uuid,
        ) -> Result<Vec<AbonnementEnregistre>, RepositoryError> {
            Ok(self
                .par_sujet
                .borrow()
                .iter()
                .filter(|(s, _)| *s == sujet_id)
                .map(|(s, a)| AbonnementEnregistre {
                    id: Uuid::new_v4(),
                    abonnement: a.clone(),
                    sujet_id: Some(*s),
                })
                .collect())
        }

        async fn supprimer_par_endpoint(&self, endpoint: &str) -> Result<bool, RepositoryError> {
            self.supprimes.borrow_mut().push(endpoint.to_string());
            self.par_sujet
                .borrow_mut()
                .retain(|(_, a)| a.endpoint != endpoint);
            Ok(true)
        }

        async fn compter(&self) -> Result<i64, RepositoryError> {
            unreachable!()
        }
    }

    #[derive(Default)]
    struct NotifieurFactice {
        envoyes: RefCell<Vec<(String, PushMessage)>>,
        disparus: Vec<String>,
        en_panne: Vec<String>,
    }

    impl PushNotifier for NotifieurFactice {
        async fn envoyer(
            &self,
            abonnement: &PushSubscription,
            message: &PushMessage,
        ) -> Result<(), PushError> {
            if self.disparus.contains(&abonnement.endpoint) {
                return Err(PushError::AbonnementExpire);
            }
            if self.en_panne.contains(&abonnement.endpoint) {
                return Err(PushError::Transport("test".into()));
            }
            self.envoyes
                .borrow_mut()
                .push((abonnement.endpoint.clone(), message.clone()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn happy_chaque_candidat_est_prevenu_dans_sa_langue() {
        // **Le défaut que la Story 9.1 corrige.** Un tour de matching réveille
        // jusqu'à dix prestataires, et rien ne dit qu'ils parlent la même
        // langue : à Bruxelles, c'est le contraire qu'il faut supposer.
        let francophone = Uuid::new_v4();
        let neerlandophone = Uuid::new_v4();
        let abonnements = AbonnementsMemoire::default();
        for sujet in [francophone, neerlandophone] {
            abonnements.par_sujet.borrow_mut().push((
                sujet,
                abonnement(&format!("https://push.example.net/{sujet}")),
            ));
        }
        let notifieur = NotifieurFactice::default();
        let langues = LanguesMemoire {
            par_compte: [(francophone, Locale::Fr), (neerlandophone, Locale::Nl)]
                .into_iter()
                .collect(),
        };

        notifier(
            &abonnements,
            &notifieur,
            &langues,
            &demande(Urgence::Haute),
            &[
                candidat(francophone, 1_200.0),
                candidat(neerlandophone, 800.0),
            ],
        )
        .await
        .unwrap();

        let envoyes = notifieur.envoyes.borrow();
        assert_eq!(envoyes.len(), 2);
        let titres: Vec<&str> = envoyes.iter().map(|(_, m)| m.titre.as_str()).collect();
        assert!(
            titres.iter().any(|t| t.starts_with("Demande")),
            "un avis en français était attendu : {titres:?}"
        );
        assert!(
            titres.iter().any(|t| t.starts_with("Aanvraag")),
            "un avis en néerlandais était attendu : {titres:?}"
        );
    }

    #[tokio::test]
    async fn edge_un_compte_sans_langue_connue_replie_sur_le_francais() {
        // Une langue illisible ne doit pas empêcher un avis de partir : mieux
        // vaut une notification dans la mauvaise langue que pas de
        // notification du tout.
        let sujet = Uuid::new_v4();
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((sujet, abonnement("https://push.example.net/inconnu")));
        let notifieur = NotifieurFactice::default();

        notifier(
            &abonnements,
            &notifieur,
            &LanguesMemoire::default(),
            &demande(Urgence::Haute),
            &[candidat(sujet, 1_200.0)],
        )
        .await
        .unwrap();

        assert_eq!(notifieur.envoyes.borrow().len(), 1);
        assert!(notifieur.envoyes.borrow()[0].1.titre.starts_with("Demande"));
    }

    /// Langues en mémoire, pour éprouver l'avis par destinataire.
    ///
    /// Vide par défaut : un compte absent replie sur le français, ce qui est
    /// exactement le comportement voulu.
    #[derive(Default)]
    struct LanguesMemoire {
        par_compte: std::collections::HashMap<Uuid, Locale>,
    }

    impl crate::ports::langue::LecteurLangue for LanguesMemoire {
        async fn langue_de(&self, compte_id: Uuid) -> Locale {
            self.par_compte
                .get(&compte_id)
                .copied()
                .unwrap_or(crate::ports::langue::LANGUE_PAR_DEFAUT)
        }
    }

    #[tokio::test]
    async fn happy_chaque_statut_a_un_avis_dans_les_trois_langues() {
        use klaar_intervention::StatutMission as S;
        let d = demande(Urgence::Haute);
        for statut in [
            S::Acceptee,
            S::EnRoute,
            S::SurPlace,
            S::Terminee,
            S::Annulee,
        ] {
            for locale in [Locale::Fr, Locale::Nl, Locale::En] {
                let m = composer_avancement(&d, statut, locale);
                assert!(!m.corps.is_empty(), "{statut:?} {locale:?}");
            }
        }
    }

    #[tokio::test]
    async fn happy_les_avis_successifs_se_remplacent() {
        // Sinon un téléphone posé sur une table affiche trois notifications
        // pour une seule intervention.
        use klaar_intervention::StatutMission as S;
        let d = demande(Urgence::Haute);
        assert_eq!(
            composer_avancement(&d, S::EnRoute, Locale::Fr).tag,
            composer_avancement(&d, S::SurPlace, Locale::Fr).tag
        );
    }

    #[tokio::test]
    async fn security_l_avis_d_avancement_ne_nomme_pas_le_prestataire() {
        // « Untel est en route » lu par-dessus une épaule dit qui vient chez
        // qui.
        use klaar_intervention::StatutMission as S;
        let d = demande(Urgence::Haute);
        let m = composer_avancement(&d, S::EnRoute, Locale::Fr);
        let tout = format!("{} {} {}", m.titre, m.corps, m.url);
        assert!(!tout.contains("Fuite"));
        assert!(!tout.contains("50.8"));
        assert!(!tout.contains(&d.demandeur_id.to_string()));
    }

    #[tokio::test]
    async fn security_l_avis_d_annulation_ne_dit_pas_pourquoi() {
        // Le motif appartient au demandeur ; le porter à dix entreprises en
        // ferait un jugement diffusé, et « trouvé ailleurs » se lit vite comme
        // un reproche.
        let d = demande(Urgence::Haute);
        let m = composer_annulation(&d, Locale::Fr);
        let tout = format!("{} {} {}", m.titre, m.corps, m.url);
        for interdit in ["FOUND_ELSEWHERE", "TOO_SLOW", "Fuite", "50.8", "4.35"] {
            assert!(
                !tout.contains(interdit),
                "le message ne doit pas dire {interdit}"
            );
        }
        assert!(!tout.contains(&d.demandeur_id.to_string()));
    }

    #[tokio::test]
    async fn happy_l_avis_d_annulation_remplace_la_notification_d_origine() {
        let d = demande(Urgence::Haute);
        assert_eq!(
            composer(&d, 500.0, Locale::Fr).tag,
            composer_annulation(&d, Locale::Fr).tag
        );
    }

    #[tokio::test]
    async fn happy_l_avis_d_annulation_est_traduit() {
        let d = demande(Urgence::Haute);
        for locale in [Locale::Fr, Locale::Nl, Locale::En] {
            let m = composer_annulation(&d, locale);
            assert!(!m.corps.is_empty(), "locale {locale:?}");
            assert!(m.titre.contains("plomberie"), "locale {locale:?}");
        }
    }

    #[tokio::test]
    async fn happy_les_candidats_notifies_apprennent_l_annulation() {
        let a = Uuid::new_v4();
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((a, abonnement("https://push.example.net/a")));
        let notifieur = NotifieurFactice::default();

        let bilan = notifier_annulation(
            &abonnements,
            &notifieur,
            &demande(Urgence::Haute),
            &[a],
            Locale::Fr,
        )
        .await
        .unwrap();
        assert_eq!(bilan.notifies, 1);
    }

    #[tokio::test]
    async fn happy_les_autres_candidats_apprennent_que_la_demande_est_prise() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((a, abonnement("https://push.example.net/a")));
        abonnements
            .par_sujet
            .borrow_mut()
            .push((b, abonnement("https://push.example.net/b")));
        let notifieur = NotifieurFactice::default();

        let bilan = notifier_match_pris(
            &abonnements,
            &notifieur,
            &demande(Urgence::Haute),
            &[a, b],
            Locale::Fr,
        )
        .await
        .unwrap();
        assert_eq!(bilan.notifies, 2);
        assert_eq!(notifieur.envoyes.borrow().len(), 2);
    }

    #[tokio::test]
    async fn happy_l_avis_de_prise_remplace_la_notification_d_origine() {
        // Même `tag` : sinon les deux s'empilent, et quelqu'un part pour une
        // intervention déjà attribuée.
        let d = demande(Urgence::Haute);
        let origine = composer(&d, 500.0, Locale::Fr);
        let avis = composer_match_pris(&d, Locale::Fr);
        assert_eq!(origine.tag, avis.tag);
        assert_ne!(origine.titre, avis.titre);
    }

    #[tokio::test]
    async fn happy_l_avis_de_prise_est_traduit() {
        let d = demande(Urgence::Haute);
        for locale in [Locale::Fr, Locale::Nl, Locale::En] {
            let m = composer_match_pris(&d, locale);
            assert!(!m.corps.is_empty(), "locale {locale:?}");
            assert!(m.titre.contains("plomberie"), "locale {locale:?}");
        }
    }

    #[tokio::test]
    async fn edge_un_candidat_sans_abonnement_n_est_pas_un_echec() {
        let bilan = notifier_match_pris(
            &AbonnementsMemoire::default(),
            &NotifieurFactice::default(),
            &demande(Urgence::Haute),
            &[Uuid::new_v4()],
            Locale::Fr,
        )
        .await
        .unwrap();
        assert_eq!(bilan.notifies, 0);
        assert_eq!(bilan.sans_abonnement, 1);
    }

    #[tokio::test]
    async fn edge_un_abonnement_disparu_est_purge_aussi_sur_l_avis_de_prise() {
        let sujet = Uuid::new_v4();
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((sujet, abonnement("https://push.example.net/mort")));
        let notifieur = NotifieurFactice {
            disparus: vec!["https://push.example.net/mort".to_string()],
            ..Default::default()
        };

        let bilan = notifier_match_pris(
            &abonnements,
            &notifieur,
            &demande(Urgence::Haute),
            &[sujet],
            Locale::Fr,
        )
        .await
        .unwrap();
        assert_eq!(bilan.purges, 1);
        assert_eq!(abonnements.supprimes.borrow().len(), 1);
    }

    #[tokio::test]
    async fn security_l_avis_de_prise_ne_dit_ni_qui_a_pris_ni_ou() {
        // Il s'affiche au même endroit que le message d'origine, sur un écran
        // verrouillé. Il ne nomme pas non plus le concurrent qui a gagné :
        // cela le désignerait à quatre autres entreprises.
        let d = demande(Urgence::Haute);
        let m = composer_match_pris(&d, Locale::Fr);
        let tout = format!("{} {} {}", m.titre, m.corps, m.url);
        assert!(!tout.contains("Fuite"));
        assert!(!tout.contains("50.8"));
        assert!(!tout.contains("4.35"));
        assert!(!tout.contains(&d.demandeur_id.to_string()));
        // Pas même l'identifiant de la Demande dans l'URL : l'avis renvoie à la
        // liste, puisqu'il n'y a plus rien à voir sur celle-ci.
        assert!(!m.url.contains(&d.id.to_string()));
    }

    #[tokio::test]
    async fn happy_chaque_candidat_abonne_recoit_le_message() {
        let sujet = Uuid::new_v4();
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((sujet, abonnement("https://push.example.net/a")));
        let notifieur = NotifieurFactice::default();

        let bilan = notifier(
            &abonnements,
            &notifieur,
            &LanguesMemoire::default(),
            &demande(Urgence::Haute),
            &[candidat(sujet, 1_200.0)],
        )
        .await
        .unwrap();

        assert_eq!(bilan.notifies, 1);
        assert_eq!(notifieur.envoyes.borrow().len(), 1);
    }

    #[tokio::test]
    async fn happy_un_candidat_a_plusieurs_appareils_les_recoit_tous() {
        let sujet = Uuid::new_v4();
        let abonnements = AbonnementsMemoire::default();
        for e in ["https://push.example.net/a", "https://push.example.net/b"] {
            abonnements
                .par_sujet
                .borrow_mut()
                .push((sujet, abonnement(e)));
        }
        let notifieur = NotifieurFactice::default();

        let bilan = notifier(
            &abonnements,
            &notifieur,
            &LanguesMemoire::default(),
            &demande(Urgence::Normale),
            &[candidat(sujet, 500.0)],
        )
        .await
        .unwrap();
        assert_eq!(bilan.notifies, 2);
    }

    #[tokio::test]
    async fn negative_un_candidat_sans_abonnement_n_est_pas_un_echec() {
        // Il verra la Demande en ouvrant l'application.
        let abonnements = AbonnementsMemoire::default();
        let notifieur = NotifieurFactice::default();
        let bilan = notifier(
            &abonnements,
            &notifieur,
            &LanguesMemoire::default(),
            &demande(Urgence::Haute),
            &[candidat(Uuid::new_v4(), 100.0)],
        )
        .await
        .unwrap();
        assert_eq!(bilan.sans_abonnement, 1);
        assert_eq!(bilan.notifies, 0);
    }

    #[tokio::test]
    async fn negative_un_abonnement_disparu_est_purge() {
        let sujet = Uuid::new_v4();
        let mort = "https://push.example.net/mort";
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((sujet, abonnement(mort)));
        let notifieur = NotifieurFactice {
            disparus: vec![mort.to_string()],
            ..Default::default()
        };

        let bilan = notifier(
            &abonnements,
            &notifieur,
            &LanguesMemoire::default(),
            &demande(Urgence::Haute),
            &[candidat(sujet, 100.0)],
        )
        .await
        .unwrap();
        assert_eq!(bilan.purges, 1);
        assert_eq!(
            abonnements.supprimes.borrow().as_slice(),
            &[mort.to_string()]
        );
    }

    #[tokio::test]
    async fn negative_une_panne_de_transport_n_interrompt_pas_le_tour() {
        // Les autres candidats n'y sont pour rien.
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let casse = "https://push.example.net/casse";
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((a, abonnement(casse)));
        abonnements
            .par_sujet
            .borrow_mut()
            .push((b, abonnement("https://push.example.net/ok")));
        let notifieur = NotifieurFactice {
            en_panne: vec![casse.to_string()],
            ..Default::default()
        };

        let bilan = notifier(
            &abonnements,
            &notifieur,
            &LanguesMemoire::default(),
            &demande(Urgence::Haute),
            &[candidat(a, 100.0), candidat(b, 200.0)],
        )
        .await
        .unwrap();
        assert_eq!(bilan.notifies, 1);
        assert_eq!(bilan.purges, 0, "une panne n'est pas une disparition");
    }

    #[tokio::test]
    async fn edge_la_distance_est_arrondie_pour_l_affichage() {
        assert_eq!(distance_lisible(0.0), "0 m");
        assert_eq!(distance_lisible(137.0), "100 m");
        assert_eq!(distance_lisible(180.0), "200 m");
        assert_eq!(distance_lisible(1_234.0), "1.2 km");
        assert_eq!(distance_lisible(4_999.0), "5.0 km");
    }

    #[tokio::test]
    async fn edge_les_trois_langues_produisent_un_message_non_vide() {
        for locale in [Locale::Fr, Locale::Nl, Locale::En] {
            for urgence in [Urgence::Basse, Urgence::Normale, Urgence::Haute] {
                let m = composer(&demande(urgence), 1_000.0, locale);
                assert!(!m.titre.is_empty(), "{locale:?}/{urgence:?}");
                assert!(!m.corps.is_empty());
            }
        }
    }

    #[tokio::test]
    async fn edge_deux_notifications_de_la_meme_demande_se_remplacent() {
        // Un `tag` par Demande : sinon dix alertes s'empilent pour une seule
        // intervention.
        let d = demande(Urgence::Haute);
        let premier = composer(&d, 100.0, Locale::Fr);
        let second = composer(&d, 200.0, Locale::Fr);
        assert_eq!(premier.tag, second.tag);
        assert_eq!(premier.tag, Some(format!("demande-{}", d.id)));
    }

    #[tokio::test]
    async fn security_le_message_ne_porte_ni_description_ni_demandeur() {
        // Une notification s'affiche sur un écran verrouillé, lisible par
        // quiconque passe à côté du téléphone. Le chiffrement de la charge
        // (RFC 8291) n'y change rien : les deux problèmes sont distincts.
        let d = demande(Urgence::Haute);
        let m = composer(&d, 1_200.0, Locale::Fr);
        let tout = format!("{} {} {}", m.titre, m.corps, m.url);

        assert!(!tout.contains("Fuite très reconnaissable"), "{tout}");
        assert!(!tout.contains("évier"));
        assert!(!tout.contains(&d.demandeur_id.to_string()));
    }

    #[tokio::test]
    async fn security_le_message_ne_porte_aucune_coordonnee() {
        // Une latitude sur un écran verrouillé situe le demandeur chez lui.
        let d = demande(Urgence::Haute);
        let m = composer(&d, 1_200.0, Locale::Fr);
        let tout = format!("{} {} {}", m.titre, m.corps, m.url);
        assert!(!tout.contains("50.8"), "{tout}");
        assert!(!tout.contains("4.35"));
    }

    #[tokio::test]
    async fn security_la_distance_affichee_ne_situe_pas_au_metre_pres() {
        // Une distance au mètre près, croisée avec la position du prestataire,
        // situerait le demandeur bien plus précisément que nécessaire.
        for metres in [137.0, 342.0, 981.0] {
            let affichee = distance_lisible(metres);
            assert!(affichee.ends_with(" m"));
            let valeur: f64 = affichee.trim_end_matches(" m").parse().unwrap();
            assert_eq!(valeur % 100.0, 0.0, "{metres} → {affichee}");
        }
    }

    #[tokio::test]
    async fn security_un_candidat_ne_recoit_pas_l_abonnement_d_un_autre() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((b, abonnement("https://push.example.net/b")));
        let notifieur = NotifieurFactice::default();

        let bilan = notifier(
            &abonnements,
            &notifieur,
            &LanguesMemoire::default(),
            &demande(Urgence::Haute),
            &[candidat(a, 100.0)],
        )
        .await
        .unwrap();
        assert_eq!(bilan.notifies, 0);
        assert_eq!(bilan.sans_abonnement, 1);
        assert!(notifieur.envoyes.borrow().is_empty());
    }

    #[tokio::test]
    async fn security_le_score_n_est_pas_communique_au_prestataire() {
        // Le score sert au classement, pas à l'affichage : le publier
        // inviterait à l'optimiser plutôt qu'à bien travailler.
        let d = demande(Urgence::Haute);
        let m = composer(&d, 1_200.0, Locale::Fr);
        let _: Score =
            klaar_matching::calculer_score(1_200.0, klaar_matching::RAYONS_METRES[0], 0.0, None);
        assert!(!format!("{} {}", m.titre, m.corps).contains("score"));
    }
}
