/**
 * Textes d'interface en trois langues (Story 9.1, FR-043).
 *
 * **Pourquoi une table et non un fichier par langue.** Trois fichiers séparés
 * laissent une clé traduite dans deux langues sur trois sans que rien ne le
 * signale ; ici, le type `Record<LocaleKlaar, ...>` sur chaque entrée fait
 * échouer la compilation si une langue manque. Une traduction oubliée est une
 * erreur de type, pas un texte français qui surgit au milieu d'une page
 * néerlandaise.
 *
 * **Le néerlandais et l'anglais sont écrits, pas générés.** Bruxelles est
 * bilingue par la loi : un néerlandophone qui lit une traduction automatique
 * approximative comprend surtout qu'on ne s'adresse pas vraiment à lui.
 *
 * **Ce qui n'est pas ici** : les messages d'erreur d'API, déjà traduits dans
 * chaque module métier (`demande.ts`, `connexion.ts`…), au plus près du code qui
 * les déclenche. Les rapatrier ici les éloignerait de leur cause.
 */
import type { LocaleKlaar } from "./inscription";

/** Les trois langues, dans l'ordre où le sélecteur les propose. */
export const LANGUES: { code: LocaleKlaar; nom: string }[] = [
  { code: "fr", nom: "Français" },
  { code: "nl", nom: "Nederlands" },
  { code: "en", nom: "English" },
];

/** Clé de persistance du choix de langue. */
const CLE_LANGUE = "klaar.langue";

type Textes = Record<LocaleKlaar, string>;

/**
 * La table des textes.
 *
 * Les clés sont nommées par écran puis par rôle du texte, pour qu'une relecture
 * de traduction puisse suivre un parcours plutôt que de sauter d'un bout à
 * l'autre de l'application.
 */
const TEXTES = {
  // --- Coquille ---
  "app.ville": {
    fr: "Bruxelles",
    nl: "Brussel",
    en: "Brussels",
  },
  "app.langue": {
    fr: "Langue",
    nl: "Taal",
    en: "Language",
  },

  // --- Navigation principale ---
  //
  // Les libellés disent une destination, pas une rubrique : « Demander un
  // dépannage » se comprend sans savoir ce que le site appelle une Demande.
  "nav.menu": {
    fr: "Navigation principale",
    nl: "Hoofdnavigatie",
    en: "Main navigation",
  },
  "nav.accueil": {
    fr: "Accueil",
    nl: "Start",
    en: "Home",
  },
  "nav.demande": {
    fr: "Demander un dépannage",
    nl: "Hulp aanvragen",
    en: "Request a call-out",
  },
  "nav.catalogue": {
    fr: "Ce que nous faisons",
    nl: "Wat wij doen",
    en: "What we do",
  },
  "nav.inscription": {
    fr: "Créer un compte",
    nl: "Account aanmaken",
    en: "Create an account",
  },
  "nav.compte": {
    fr: "Mon compte",
    nl: "Mijn account",
    en: "My account",
  },
  // Deux libellés pour la même page : un visiteur y va pour savoir ce que
  // c'est, un prestataire connecté pour travailler. Le front ne peut pas
  // distinguer mieux — voir `navigation.ts`.
  "nav.prestataire_visiteur": {
    fr: "Je suis prestataire",
    nl: "Ik ben dienstverlener",
    en: "I am a provider",
  },
  "nav.prestataire": {
    fr: "Espace prestataire",
    nl: "Dienstverlenersruimte",
    en: "Provider area",
  },
  "nav.ops": {
    fr: "Exploitation",
    nl: "Exploitatie",
    en: "Operations",
  },

  // --- Actions communes ---
  "commun.attendez": {
    fr: "Un instant…",
    nl: "Een ogenblik…",
    en: "One moment…",
  },
  "commun.rafraichir": {
    fr: "Rafraîchir",
    nl: "Vernieuwen",
    en: "Refresh",
  },
  "commun.annuler": {
    fr: "Annuler",
    nl: "Annuleren",
    en: "Cancel",
  },
  "commun.revenir": {
    fr: "Revenir",
    nl: "Terug",
    en: "Go back",
  },
  "commun.connexion_requise": {
    fr: "Cette page demande d'être connecté.",
    nl: "Voor deze pagina moet u aangemeld zijn.",
    en: "This page requires you to be signed in.",
  },
  "commun.me_connecter": {
    fr: "Me connecter",
    nl: "Aanmelden",
    en: "Sign in",
  },

  // --- Suivi d'une Demande ---
  "suivi.introuvable": {
    fr: "Demande introuvable.",
    nl: "Aanvraag niet gevonden.",
    en: "Request not found.",
  },
  "suivi.zone": {
    fr: "zone de",
    nl: "zone van",
    en: "area of",
  },
  "suivi.elargie": {
    fr: "élargie {n} fois sur 3",
    nl: "{n} van 3 keer uitgebreid",
    en: "widened {n} of 3 times",
  },
  "suivi.devis_recu": {
    fr: "Devis reçu",
    nl: "Ontvangen offerte",
    en: "Quote received",
  },

  // --- Suivi géolocalisé (FR-019) ---
  "trajet.en_route": {
    fr: "Position partagée il y a moins d'une minute",
    nl: "Positie minder dan een minuut geleden gedeeld",
    en: "Location shared less than a minute ago",
  },
  "trajet.hors_zone": {
    fr: "Le prestataire est loin de l'adresse",
    nl: "De vakman is ver van het adres",
    en: "The provider is far from the address",
  },
  "trajet.perdue": {
    // Ne dit pas « panne » : le prestataire peut n'avoir pas consenti, ou
    // traverser un tunnel.
    fr: "Position non partagée pour le moment",
    nl: "Positie wordt momenteel niet gedeeld",
    en: "Location not shared at the moment",
  },
  "trajet.arrete": {
    fr: "Suivi terminé",
    nl: "Opvolging beëindigd",
    en: "Tracking finished",
  },
  "trajet.precision": {
    fr: "à 50 m près",
    nl: "tot op 50 m nauwkeurig",
    en: "accurate to 50 m",
  },

  // --- Espace prestataire ---
  "prestataire.partager_position": {
    fr: "Partager ma position",
    nl: "Mijn positie delen",
    en: "Share my location",
  },
  "prestataire.arreter_partage": {
    fr: "Arrêter de partager ma position",
    nl: "Stoppen met mijn positie te delen",
    en: "Stop sharing my location",
  },
  "prestataire.partage_explication": {
    fr: "Partager votre position pendant le trajet aide le demandeur à savoir quand vous arrivez. La position est arrondie à cinquante mètres, n'est visible que de lui, et tout est effacé vingt-quatre heures après l'intervention. Vous pouvez arrêter à tout moment.",
    nl: "Uw positie delen tijdens de rit helpt de aanvrager te weten wanneer u aankomt. De positie wordt afgerond tot vijftig meter, is alleen voor hem zichtbaar, en alles wordt vierentwintig uur na de opdracht gewist. U kunt op elk moment stoppen.",
    en: "Sharing your location while travelling helps the requester know when you will arrive. The location is rounded to fifty metres, is visible only to them, and everything is erased twenty-four hours after the job. You can stop at any time.",
  },
  "prestataire.geoloc_absente": {
    fr: "Cet appareil ne sait pas donner sa position.",
    nl: "Dit toestel kan zijn positie niet doorgeven.",
    en: "This device cannot report its location.",
  },
  "prestataire.intervention_close": {
    fr: "Cette intervention est close.",
    nl: "Deze opdracht is afgesloten.",
    en: "This job is closed.",
  },
  "prestataire.revenir_demandes": {
    fr: "Revenir aux Demandes",
    nl: "Terug naar de aanvragen",
    en: "Back to requests",
  },

  // --- État de connexion (Story 0.2) ---
  "connexion.verification": {
    fr: "Vérification de la connexion…",
    nl: "Verbinding controleren…",
    en: "Checking the connection…",
  },
  "connexion.en_ligne": {
    fr: "En ligne",
    nl: "Online",
    en: "Online",
  },
  "connexion.hors_ligne": {
    fr: "Hors ligne, vos saisies sont conservées",
    nl: "Offline, uw invoer wordt bewaard",
    en: "Offline, your entries are kept",
  },
  "connexion.en_attente": {
    fr: "{n} en attente d'envoi",
    nl: "{n} wachten op verzending",
    en: "{n} waiting to be sent",
  },
  // **Pas de marqueur de pluriel en variable.** La version précédente passait
  // un « {s} » calculé côté français : une lettre de pluriel n'est pas une
  // donnée, et les autres langues n'en veulent pas au même endroit — le
  // néerlandais n'accorde pas ce participe du tout. La forme entre parenthèses
  // est celle que le reste de l'application emploie déjà.
  "connexion.refusees": {
    fr: "{n} refusée(s)",
    nl: "{n} geweigerd",
    en: "{n} rejected",
  },

  // --- Vérification d'adresse (Story 1.2) ---
  "verification.en_cours": {
    fr: "Confirmation en cours…",
    nl: "Bevestiging bezig…",
    en: "Confirming…",
  },
  "verification.accueil": {
    fr: "Retour à l'accueil",
    nl: "Terug naar de startpagina",
    en: "Back to the home page",
  },
  "verification.recommencer": {
    fr: "Recommencer l'inscription",
    nl: "Opnieuw registreren",
    en: "Start registration again",
  },

  // --- Catalogue (Story 2.x) ---
  "catalogue.chargement": {
    fr: "Chargement du catalogue…",
    nl: "Catalogus laden…",
    en: "Loading the catalogue…",
  },
  "catalogue.vide": {
    fr: "Le catalogue est vide pour le moment.",
    nl: "De catalogus is momenteel leeg.",
    en: "The catalogue is empty for now.",
  },

  // --- Notifications (Story 0.12) ---
  "push.non_supporte": {
    fr: "Ce navigateur ne délivre pas de notifications. Sur iPhone, ajoutez d'abord Klaar à votre écran d'accueil : Safari ne les délivre qu'aux applications installées.",
    nl: "Deze browser levert geen meldingen. Voeg Klaar op een iPhone eerst toe aan uw beginscherm: Safari levert ze alleen aan geïnstalleerde toepassingen.",
    en: "This browser does not deliver notifications. On iPhone, first add Klaar to your home screen: Safari only delivers them to installed apps.",
  },
  "push.non_configure": {
    fr: "Les notifications ne sont pas activées sur ce déploiement.",
    nl: "Meldingen zijn niet ingeschakeld op deze installatie.",
    en: "Notifications are not enabled on this deployment.",
  },
  "push.refuse": {
    fr: "Les notifications sont bloquées pour ce site. Le rétablir se fait dans les réglages du navigateur, pas depuis cette page.",
    nl: "Meldingen zijn geblokkeerd voor deze site. Dat herstelt u in de instellingen van uw browser, niet vanaf deze pagina.",
    en: "Notifications are blocked for this site. Re-enabling them happens in your browser settings, not from this page.",
  },
  "push.desactiver": {
    fr: "Désactiver les notifications",
    nl: "Meldingen uitschakelen",
    en: "Turn off notifications",
  },
  "push.activer": {
    fr: "Recevoir les notifications",
    nl: "Meldingen ontvangen",
    en: "Receive notifications",
  },

  // --- Connexion (Story 1.3) ---
  "connexion.reprise": {
    fr: "Reprise de session…",
    nl: "Sessie hervatten…",
    en: "Resuming session…",
  },
  "connexion.connecte": {
    fr: "Vous êtes connecté. La session se renouvelle d'elle-même avant d'expirer.",
    nl: "U bent aangemeld. De sessie vernieuwt zichzelf voordat ze verloopt.",
    en: "You are signed in. The session renews itself before it expires.",
  },
  "connexion.deconnecter": {
    fr: "Me déconnecter",
    nl: "Afmelden",
    en: "Sign out",
  },
  "champ.email": {
    fr: "Adresse email",
    nl: "E-mailadres",
    en: "Email address",
  },
  "champ.mot_de_passe": {
    fr: "Mot de passe",
    nl: "Wachtwoord",
    en: "Password",
  },

  // --- Inscription (Story 1.1) ---
  "inscription.aide_mot_de_passe": {
    fr: "Au moins {n} caractères. Aucune règle de composition : une phrase que vous retenez vaut mieux qu'un sigle que vous oublierez.",
    nl: "Minstens {n} tekens. Geen samenstellingsregels: een zin die u onthoudt is beter dan een afkorting die u vergeet.",
    en: "At least {n} characters. No composition rules: a sentence you remember beats an acronym you will forget.",
  },
  "inscription.encore_caracteres": {
    fr: "Encore {n} caractère(s).",
    nl: "Nog {n} teken(s).",
    en: "{n} more character(s).",
  },
  "inscription.creer": {
    fr: "Créer mon compte",
    nl: "Mijn account aanmaken",
    en: "Create my account",
  },

  // --- Disponibilité du prestataire (Story 3.7) ---
  "dispo.illisible": {
    fr: "Votre disponibilité n'a pas pu être lue.",
    nl: "Uw beschikbaarheid kon niet worden gelezen.",
    en: "Your availability could not be read.",
  },
  "dispo.sollicitable": {
    fr: "Vous recevez les Demandes de vos secteurs.",
    nl: "U ontvangt de aanvragen uit uw sectoren.",
    en: "You receive requests in your sectors.",
  },
  "dispo.pause": {
    fr: "Me mettre en pause",
    nl: "Mij op pauze zetten",
    en: "Pause me",
  },
  "dispo.reprendre": {
    fr: "Me remettre en service",
    nl: "Mij weer beschikbaar stellen",
    en: "Put me back in service",
  },
  "dispo.titre_rayon": {
    fr: "Jusqu'où je me déplace",
    nl: "Hoe ver ik rijd",
    en: "How far I travel",
  },
  "dispo.explication_rayon": {
    fr: "Au-delà de cette distance, les Demandes ne vous seront pas proposées. C'est votre limite à vous ; celle de la recherche peut être plus courte.",
    nl: "Voorbij die afstand worden u geen aanvragen voorgesteld. Dit is uw eigen grens; die van de zoekopdracht kan korter zijn.",
    en: "Beyond that distance, requests will not be offered to you. This is your own limit; the search radius may be shorter.",
  },
  "dispo.rayon": {
    fr: "Rayon d'intervention : {km} km",
    nl: "Werkstraal: {km} km",
    en: "Service radius: {km} km",
  },
  "dispo.enregistrer": {
    fr: "Enregistrer",
    nl: "Opslaan",
    en: "Save",
  },
  // Les raisons du silence : chacune dit ce qui bloque, et ce qui ne le lèvera
  // pas. « Se remettre en service » ne réactive pas un compte suspendu, et le
  // laisser croire ferait cliquer pour rien.
  "silence.kyc": {
    fr: "Votre inscription attend son contrôle. Vous ne recevrez rien avant.",
    nl: "Uw inschrijving wacht op controle. Voordien ontvangt u niets.",
    en: "Your registration is awaiting review. You will receive nothing before that.",
  },
  "silence.suspendu": {
    fr: "Votre compte est suspendu. Se remettre en service ne le réactive pas.",
    nl: "Uw account is geschorst. Uzelf weer beschikbaar stellen heractiveert het niet.",
    en: "Your account is suspended. Putting yourself back in service does not reactivate it.",
  },
  "silence.pause": {
    fr: "Vous êtes en pause : aucune Demande ne vous parvient.",
    nl: "U staat op pauze: er bereikt u geen enkele aanvraag.",
    en: "You are paused: no request reaches you.",
  },
  "silence.occupe": {
    fr: "Une intervention est en cours. Vous recevrez à nouveau des Demandes quand elle sera terminée.",
    nl: "Er loopt een opdracht. U ontvangt opnieuw aanvragen zodra ze afgerond is.",
    en: "A job is under way. You will receive requests again once it is finished.",
  },

  // --- Messagerie (Story 6.1) ---
  "conversation.titre": {
    fr: "Messages",
    nl: "Berichten",
    en: "Messages",
  },
  "conversation.vide": {
    fr: "Aucun message. Vous pouvez écrire ici tout ce qui concerne l'intervention : c'est ce qui fait preuve en cas de désaccord.",
    nl: "Geen berichten. U kunt hier alles schrijven over de opdracht: dat geldt als bewijs bij onenigheid.",
    en: "No messages. You can write anything about the job here: it is what counts as evidence if you disagree.",
  },
  "conversation.votre_message": {
    fr: "Votre message",
    nl: "Uw bericht",
    en: "Your message",
  },
  "conversation.envoyer": {
    fr: "Envoyer",
    nl: "Versturen",
    en: "Send",
  },

  // --- Effacement du compte (Story 1.7, RGPD art. 17) ---
  "compte.effacement_programme": {
    fr: "L'effacement de votre compte est programmé dans {n} jours. Vos données personnelles seront supprimées à cette échéance. Vous pouvez encore changer d'avis jusque-là.",
    nl: "Het wissen van uw account is over {n} dagen gepland. Uw persoonsgegevens worden dan verwijderd. U kunt tot dan nog van gedachten veranderen.",
    en: "Your account is scheduled for deletion in {n} days. Your personal data will be removed then. You can still change your mind until that date.",
  },
  "compte.annuler_effacement": {
    fr: "Annuler l'effacement",
    nl: "Het wissen annuleren",
    en: "Cancel the deletion",
  },
  "compte.ouvrir_effacement": {
    fr: "Effacer mon compte",
    nl: "Mijn account wissen",
    en: "Delete my account",
  },
  "compte.ce_qui_part": {
    fr: "Cette demande supprimera votre adresse, votre mot de passe, vos sessions et vos abonnements aux notifications, après un délai de trente jours pendant lequel vous pourrez encore l'annuler.",
    nl: "Dit verzoek verwijdert uw adres, uw wachtwoord, uw sessies en uw meldingsabonnementen, na een termijn van dertig dagen waarin u het nog kunt annuleren.",
    en: "This request will remove your address, your password, your sessions and your notification subscriptions, after a thirty-day period during which you can still cancel it.",
  },
  "compte.ce_qui_reste": {
    fr: "Le journal d'audit, lui, est conservé : il ne porte ni votre adresse ni aucun contenu, seulement la trace horodatée que ce droit a été exercé.",
    nl: "Het auditlogboek blijft wel bewaard: het bevat noch uw adres, noch enige inhoud, alleen het tijdstempel dat dit recht is uitgeoefend.",
    en: "The audit log is kept: it holds neither your address nor any content, only the timestamped trace that this right was exercised.",
  },
  // Coupé en deux plutôt que rendu en `{@html}` : le mot de confirmation est une
  // constante du code, donc sans risque, mais introduire du HTML brut pour un
  // `<code>` ouvrirait un chemin dont le prochain usage, lui, portera peut-être
  // une saisie. Le mot se place au milieu de la phrase dans les trois langues.
  "compte.recopier_avant": {
    fr: "Recopiez",
    nl: "Typ",
    en: "Type",
  },
  "compte.recopier_apres": {
    fr: "pour confirmer",
    nl: "over ter bevestiging",
    en: "to confirm",
  },
  "compte.effacer_definitivement": {
    fr: "Effacer définitivement",
    nl: "Definitief wissen",
    en: "Delete permanently",
  },
  "compte.renoncer": {
    fr: "Renoncer",
    nl: "Afzien",
    en: "Give up",
  },

  // --- Soumission d'une Demande (Story 3.1) ---
  "demande.compte_requis": {
    fr: "Faire une demande suppose un compte.",
    nl: "Een aanvraag doen vereist een account.",
    en: "Making a request requires an account.",
  },
  "demande.en_file": {
    fr: "Aucune connexion. Votre demande est conservée sur cet appareil et partira dès que le réseau reviendra. Gardez cette page ouverte.",
    nl: "Geen verbinding. Uw aanvraag wordt op dit toestel bewaard en vertrekt zodra het netwerk terug is. Houd deze pagina open.",
    en: "No connection. Your request is kept on this device and will be sent as soon as the network returns. Keep this page open.",
  },
  "demande.rien_envoye": {
    fr: "Rien n'a encore été envoyé : aucun prestataire n'a été prévenu pour l'instant.",
    nl: "Er is nog niets verstuurd: voorlopig is geen enkele vakman verwittigd.",
    en: "Nothing has been sent yet: no provider has been notified so far.",
  },
  "demande.doublon": {
    fr: "Vous aviez déjà une demande en cours pour ce secteur, ici même. C'est elle qui est en train d'être diffusée.",
    nl: "U had hier al een lopende aanvraag voor deze sector. Die wordt momenteel verspreid.",
    en: "You already had a request under way for this sector, at this very place. That is the one being broadcast.",
  },
  "demande.diffusee": {
    fr: "Votre demande est diffusée aux prestataires disponibles.",
    nl: "Uw aanvraag wordt verspreid onder de beschikbare vakmensen.",
    en: "Your request is being broadcast to available providers.",
  },
  "demande.aucun_candidat": {
    fr: "Aucun prestataire disponible dans la zone pour l'instant. Vous pourrez élargir la recherche depuis le suivi.",
    nl: "Momenteel geen beschikbare vakman in de zone. U kunt de zoekopdracht vanuit de opvolging uitbreiden.",
    en: "No provider available in the area right now. You will be able to widen the search from the tracking page.",
  },
  // Deux nombres et non un : un prestataire retenu sans abonnement aux
  // notifications verra la Demande en ouvrant l'application. Les confondre
  // ferait croire que dix personnes ont été réveillées alors que personne n'a
  // rien reçu.
  "demande.candidats": {
    fr: "{c} prestataire(s) retenu(s), dont {n} prévenu(s) par notification.",
    nl: "{c} vakman/vakmensen weerhouden, waarvan {n} verwittigd via melding.",
    en: "{c} provider(s) selected, of whom {n} notified by push.",
  },
  "demande.suivre": {
    fr: "Suivre ma demande",
    nl: "Mijn aanvraag opvolgen",
    en: "Track my request",
  },
  "demande.secteur": {
    fr: "Secteur",
    nl: "Sector",
    en: "Sector",
  },
  "demande.choisissez": {
    fr: "Choisissez…",
    nl: "Kies…",
    en: "Choose…",
  },
  "demande.que_se_passe_t_il": {
    fr: "Que se passe-t-il ?",
    nl: "Wat is er aan de hand?",
    en: "What is happening?",
  },
  "demande.restants": {
    fr: "{n} caractères restants",
    nl: "nog {n} tekens",
    en: "{n} characters left",
  },
  "demande.urgence": {
    fr: "Urgence",
    nl: "Dringendheid",
    en: "Urgency",
  },
  "urgence.basse": {
    fr: "Peut attendre",
    nl: "Kan wachten",
    en: "Can wait",
  },
  "urgence.normale": {
    fr: "Dans la journée",
    nl: "Vandaag nog",
    en: "Within the day",
  },
  "urgence.haute": {
    fr: "Tout de suite",
    nl: "Onmiddellijk",
    en: "Right away",
  },
  "demande.envoyer": {
    fr: "Envoyer ma demande",
    nl: "Mijn aanvraag versturen",
    en: "Send my request",
  },
  "demande.position_requise": {
    fr: "Votre position sera demandée à l'envoi : sans elle, aucun prestataire ne peut être averti.",
    nl: "Bij het versturen wordt uw positie gevraagd: zonder die kan geen enkele vakman verwittigd worden.",
    en: "Your location will be requested when sending: without it, no provider can be alerted.",
  },

  // --- Suivi d'une Demande, suite (Stories 4.x) ---
  "suivi.km": {
    fr: "{secteur} · zone de {km} km",
    nl: "{secteur} · zone van {km} km",
    en: "{secteur} · area of {km} km",
  },
  "devis.ttc": {
    fr: "TTC",
    nl: "incl. btw",
    en: "incl. VAT",
  },
  "devis.detail": {
    fr: "({htva} hors TVA + {tva} de TVA à {taux} %)",
    nl: "({htva} excl. btw + {tva} btw aan {taux} %)",
    en: "({htva} excl. VAT + {tva} VAT at {taux} %)",
  },
  "devis.delai": {
    fr: "Intervention annoncée sous {delai}.",
    nl: "Interventie aangekondigd binnen {delai}.",
    en: "Job announced within {delai}.",
  },
  "devis.accepter": {
    fr: "J'accepte ce devis",
    nl: "Ik aanvaard deze offerte",
    en: "I accept this quote",
  },
  "devis.refuser": {
    fr: "Je refuse",
    nl: "Ik weiger",
    en: "I decline",
  },
  "devis.motif_refus": {
    fr: "Si vous refusez, pourquoi (facultatif)",
    nl: "Als u weigert, waarom (optioneel)",
    en: "If you decline, why (optional)",
  },
  "commun.sans_motif": {
    fr: "Sans motif",
    nl: "Zonder reden",
    en: "No reason given",
  },
  // Invariant §10.2 : le prix vient du prestataire, et l'écran le dit plutôt
  // que de le laisser deviner.
  "devis.prix_libre": {
    fr: "C'est le prestataire qui fixe son prix. Klaar ne le lui suggère pas et ne le corrige pas.",
    nl: "De vakman bepaalt zelf zijn prijs. Klaar stelt die niet voor en corrigeert die niet.",
    en: "The provider sets their own price. Klaar neither suggests nor corrects it.",
  },
  // Le paiement relève de l'Epic 5 : le taire laisserait attendre un
  // prélèvement qui n'arrive pas.
  "devis.reglement_direct": {
    fr: "L'accord est enregistré ici ; le règlement se fait pour l'instant directement avec le prestataire.",
    nl: "De overeenkomst wordt hier vastgelegd; de betaling verloopt voorlopig rechtstreeks met de vakman.",
    en: "The agreement is recorded here; payment currently happens directly with the provider.",
  },

  // --- Notation (Story 7.1) ---
  "note.question": {
    fr: "Comment s'est passée l'intervention ?",
    nl: "Hoe is de opdracht verlopen?",
    en: "How did the job go?",
  },
  "note.etoiles": {
    fr: "De 1 à 5 étoiles",
    nl: "Van 1 tot 5 sterren",
    en: "From 1 to 5 stars",
  },
  "note.commentaire": {
    fr: "Un mot (facultatif)",
    nl: "Een woordje (optioneel)",
    en: "A word (optional)",
  },
  "note.envoyer": {
    fr: "Envoyer ma note",
    nl: "Mijn beoordeling versturen",
    en: "Send my rating",
  },
  "note.cachee": {
    fr: "Votre note reste cachée tant que le prestataire n'a pas donné la sienne : les deux s'affichent ensemble, pour que personne n'ajuste la sienne en fonction de l'autre.",
    nl: "Uw beoordeling blijft verborgen tot de vakman de zijne heeft gegeven: beide verschijnen samen, zodat niemand de zijne aanpast aan die van de ander.",
    en: "Your rating stays hidden until the provider has given theirs: both appear together, so that neither adjusts to the other.",
  },
  "note.merci": {
    fr: "Merci. Votre note s'affichera quand le prestataire aura donné la sienne.",
    nl: "Bedankt. Uw beoordeling verschijnt zodra de vakman de zijne heeft gegeven.",
    en: "Thank you. Your rating will appear once the provider has given theirs.",
  },

  // --- Litige (Story 7.2) ---
  "litige.ouvrir_details": {
    fr: "L'intervention s'est mal passée ?",
    nl: "Is de opdracht slecht verlopen?",
    en: "Did the job go badly?",
  },
  "litige.explication": {
    fr: "Vous pouvez ouvrir un litige pendant quatorze jours. Il sera examiné, et le prestataire pourra donner sa version.",
    nl: "U kunt veertien dagen lang een geschil openen. Het wordt onderzocht, en de vakman kan zijn versie geven.",
    en: "You can open a dispute for fourteen days. It will be examined, and the provider will be able to give their version.",
  },
  "litige.motif": {
    fr: "Que s'est-il passé",
    nl: "Wat is er gebeurd",
    en: "What happened",
  },
  "litige.recit": {
    fr: "Racontez, en quelques phrases",
    nl: "Vertel het in enkele zinnen",
    en: "Tell us, in a few sentences",
  },
  "litige.ouvrir": {
    fr: "Ouvrir un litige",
    nl: "Een geschil openen",
    en: "Open a dispute",
  },
  "litige.ouvert": {
    fr: "Votre litige est ouvert. Il sera examiné, et vous serez tenu au courant.",
    nl: "Uw geschil is geopend. Het wordt onderzocht, en u wordt op de hoogte gehouden.",
    en: "Your dispute is open. It will be examined, and you will be kept informed.",
  },

  // --- Annulation d'intervention (Story 4.7) ---
  "arret.motif": {
    fr: "Annuler l'intervention (facultatif : pourquoi)",
    nl: "De opdracht annuleren (optioneel: waarom)",
    en: "Cancel the job (optional: why)",
  },
  "arret.bouton": {
    fr: "Annuler l'intervention",
    nl: "De opdracht annuleren",
    en: "Cancel the job",
  },
  "arret.forfait": {
    fr: "Le prestataire est déjà sur place : 30 € lui resteront pour son déplacement, le reste vous est rendu.",
    nl: "De vakman is al ter plaatse: 30 € blijft voor zijn verplaatsing, de rest krijgt u terug.",
    en: "The provider is already on site: 30 € will stay with them for the trip, the rest is returned to you.",
  },

  // --- Validation (Story 4.6) ---
  "validation.question": {
    fr: "Le prestataire a déclaré avoir terminé. Si c'est bien le cas, confirmez-le : c'est ce qui déclenche son paiement.",
    nl: "De vakman heeft verklaard klaar te zijn. Als dat klopt, bevestig het: dat zet zijn betaling in gang.",
    en: "The provider has declared the job finished. If that is so, confirm it: that is what triggers their payment.",
  },
  "validation.bouton": {
    fr: "L'intervention est bien terminée",
    nl: "De opdracht is inderdaad afgerond",
    en: "The job is indeed finished",
  },
  "validation.automatique": {
    fr: "Sans réponse de votre part, elle sera validée automatiquement dans les 72 heures.",
    nl: "Zonder antwoord van u wordt ze binnen 72 uur automatisch gevalideerd.",
    en: "Without an answer from you, it will be validated automatically within 72 hours.",
  },

  // --- Élargissement et retrait (Stories 3.6, 3.5) ---
  "suivi.elargir": {
    fr: "Élargir la zone de recherche",
    nl: "De zoekzone uitbreiden",
    en: "Widen the search area",
  },
  "suivi.motif_retrait": {
    fr: "Pourquoi (facultatif)",
    nl: "Waarom (optioneel)",
    en: "Why (optional)",
  },
  "suivi.retirer": {
    fr: "Retirer ma demande",
    nl: "Mijn aanvraag intrekken",
    en: "Withdraw my request",
  },

  // --- Écran prestataire, suite (Story 4.10) ---
  "pro.intervention_en_cours": {
    fr: "Intervention en cours — {secteur}",
    nl: "Lopende opdracht — {secteur}",
    en: "Job under way — {secteur}",
  },
  "pro.urgence_adresse": {
    fr: "Urgence : {urgence} · Adresse :",
    nl: "Dringendheid: {urgence} · Adres:",
    en: "Urgency: {urgence} · Address:",
  },
  "pro.devis_envoye": {
    fr: "Devis envoyé",
    nl: "Verstuurde offerte",
    en: "Quote sent",
  },
  "pro.devis_detail": {
    fr: "({htva} HTVA + {tva} de TVA à {taux} %)",
    nl: "({htva} excl. btw + {tva} btw aan {taux} %)",
    en: "({htva} excl. VAT + {tva} VAT at {taux} %)",
  },
  "pro.intervention_sous": {
    fr: "Intervention sous {delai}",
    nl: "Interventie binnen {delai}",
    en: "Job within {delai}",
  },
  "pro.plafond_devis": {
    fr: "Trois devis ont déjà été envoyés pour cette intervention. Un de plus l'annulerait.",
    nl: "Er zijn al drie offertes verstuurd voor deze opdracht. Nog één zou ze annuleren.",
    en: "Three quotes have already been sent for this job. One more would cancel it.",
  },
  "pro.envoyer_devis_titre": {
    fr: "Envoyer un devis",
    nl: "Een offerte versturen",
    en: "Send a quote",
  },
  // Invariant §10.2 : aucune suggestion de prix, et l'écran le dit.
  "pro.prix_libre": {
    fr: "Vous fixez votre prix. Klaar n'en propose aucun, n'en suggère aucun et n'en corrige aucun. Il vous reste {n} envoi(s).",
    nl: "U bepaalt uw prijs. Klaar stelt er geen voor, suggereert er geen en corrigeert er geen. U hebt nog {n} verzending(en).",
    en: "You set your price. Klaar proposes none, suggests none and corrects none. You have {n} sending(s) left.",
  },
  "pro.montant_htva": {
    fr: "Montant hors TVA, en euros",
    nl: "Bedrag exclusief btw, in euro",
    en: "Amount excluding VAT, in euros",
  },
  "pro.taux_tva": {
    fr: "Taux de TVA",
    nl: "Btw-tarief",
    en: "VAT rate",
  },
  "pro.taux_normal": {
    fr: "21 % — taux normal",
    nl: "21 % — normaal tarief",
    en: "21 % — standard rate",
  },
  "pro.taux_logement": {
    fr: "6 % — logement de plus de 5 ans",
    nl: "6 % — woning ouder dan 5 jaar",
    en: "6 % — dwelling over 5 years old",
  },
  "pro.taux_isolation": {
    fr: "12 % — isolation thermique",
    nl: "12 % — thermische isolatie",
    en: "12 % — thermal insulation",
  },
  "pro.preuve_taux": {
    fr: "Preuve du taux réduit",
    nl: "Bewijs van het verlaagde tarief",
    en: "Evidence for the reduced rate",
  },
  "pro.delai_minutes": {
    fr: "Délai d'intervention, en minutes",
    nl: "Interventietermijn, in minuten",
    en: "Response time, in minutes",
  },
  "pro.note_demandeur": {
    fr: "Note pour le demandeur",
    nl: "Nota voor de aanvrager",
    en: "Note for the requester",
  },
  "pro.apercu": {
    fr: "Le demandeur verra {ttc} TTC.",
    nl: "De aanvrager ziet {ttc} incl. btw.",
    en: "The requester will see {ttc} incl. VAT.",
  },
  "pro.envoyer_devis": {
    fr: "Envoyer le devis",
    nl: "De offerte versturen",
    en: "Send the quote",
  },
  "pro.motif_desistement": {
    fr: "Si vous ne pouvez plus venir, pourquoi",
    nl: "Als u niet meer kunt komen, waarom",
    en: "If you can no longer come, why",
  },
  "pro.se_desister": {
    fr: "Je ne peux plus assurer cette intervention",
    nl: "Ik kan deze opdracht niet meer uitvoeren",
    en: "I can no longer take this job",
  },
  "pro.suspension": {
    fr: "Trois désistements en trente jours suspendent votre compte pendant une semaine.",
    nl: "Drie afzeggingen in dertig dagen schorsen uw account gedurende een week.",
    en: "Three withdrawals in thirty days suspend your account for a week.",
  },
  "pro.aucune_demande": {
    fr: "Aucune Demande en attente. Elles apparaissent ici pendant les trente secondes qui suivent leur diffusion.",
    nl: "Geen wachtende aanvragen. Ze verschijnen hier gedurende de dertig seconden na hun verspreiding.",
    en: "No requests waiting. They appear here during the thirty seconds following their broadcast.",
  },
  "pro.urgence_restant": {
    fr: "Urgence : {urgence} · Encore {s} s",
    nl: "Dringendheid: {urgence} · Nog {s} s",
    en: "Urgency: {urgence} · {s} s left",
  },
  // L'asymétrie du FR-013 : l'adresse n'est révélée qu'à qui prend la Mission.
  "pro.adresse_apres": {
    fr: "L'adresse exacte vous sera donnée si vous prenez cette intervention.",
    nl: "Het exacte adres krijgt u pas als u deze opdracht aanneemt.",
    en: "The exact address will be given to you if you take this job.",
  },
  "pro.je_prends": {
    fr: "Je prends",
    nl: "Ik neem ze aan",
    en: "I take it",
  },

  // --- Vocabulaires fermés : motifs (FR-014, FR-017, FR-022, FR-034) ---
  //
  // Les listes de motifs portent des **clés** et non des libellés : un motif
  // écrit en français dans le code ne se traduit jamais, et les vocabulaires
  // fermés sont précisément ce qu'un demandeur néerlandophone lit le plus.
  "motif.trop_cher": { fr: "Trop cher", nl: "Te duur", en: "Too expensive" },
  "motif.trop_long": {
    fr: "Trop long à venir",
    nl: "Te lang wachten",
    en: "Takes too long to arrive",
  },
  "motif.plus_besoin": {
    fr: "Je n'en ai plus besoin",
    nl: "Ik heb het niet meer nodig",
    en: "I no longer need it",
  },
  "motif.autre": { fr: "Autre", nl: "Andere", en: "Other" },
  "motif.mal_fait": {
    fr: "Le travail est mal fait",
    nl: "Het werk is slecht uitgevoerd",
    en: "The work is badly done",
  },
  "motif.rien_fait": {
    fr: "Rien n'a été fait",
    nl: "Er is niets gedaan",
    en: "Nothing was done",
  },
  "motif.montant_conteste": {
    fr: "Le montant ne correspond pas à ce qui était convenu",
    nl: "Het bedrag komt niet overeen met wat was afgesproken",
    en: "The amount does not match what was agreed",
  },
  "motif.pas_d_acces": {
    fr: "Personne ne peut ouvrir",
    nl: "Niemand kan opendoen",
    en: "Nobody can open the door",
  },
  "motif.desaccord": {
    fr: "Désaccord sur le travail à faire",
    nl: "Onenigheid over het uit te voeren werk",
    en: "Disagreement about the work to be done",
  },
  "motif.regle_seul": {
    fr: "Le problème s'est réglé tout seul",
    nl: "Het probleem is vanzelf opgelost",
    en: "The problem sorted itself out",
  },
  "motif.trouve_ailleurs": {
    fr: "J'ai trouvé quelqu'un d'autre",
    nl: "Ik heb iemand anders gevonden",
    en: "I found someone else",
  },
  "motif.erreur": {
    fr: "Je me suis trompé",
    nl: "Ik heb me vergist",
    en: "I made a mistake",
  },
  "motif.pro_empeche": {
    fr: "Je ne peux plus venir",
    nl: "Ik kan niet meer komen",
    en: "I can no longer come",
  },
  "motif.pro_hors_competence": {
    fr: "Ce n'est pas de ma compétence",
    nl: "Dit valt niet onder mijn vakgebied",
    en: "This is outside my skills",
  },
  "motif.pro_inaccessible": {
    fr: "Impossible d'accéder au lieu",
    nl: "Onmogelijk om ter plaatse te geraken",
    en: "Cannot access the place",
  },

  // --- Statuts de devis (FR-016, FR-017) ---
  "devis.expire": {
    fr: "Ce devis a expiré sans réponse.",
    nl: "Deze offerte is zonder antwoord vervallen.",
    en: "This quote expired without an answer.",
  },
  "devis.vous_attend": {
    fr: "Un devis vous attend.",
    nl: "Er wacht een offerte op u.",
    en: "A quote is waiting for you.",
  },
  "devis.accepte": {
    fr: "Vous avez accepté ce devis.",
    nl: "U hebt deze offerte aanvaard.",
    en: "You accepted this quote.",
  },
  "devis.refuse": {
    fr: "Vous avez refusé ce devis.",
    nl: "U hebt deze offerte geweigerd.",
    en: "You declined this quote.",
  },

  // --- Statuts, côté demandeur (FR-015, FR-018) ---
  "statut.pris_par": {
    fr: "{nom} a pris votre demande",
    nl: "{nom} heeft uw aanvraag aangenomen",
    en: "{nom} has taken your request",
  },
  "statut.pris_anonyme": {
    fr: "Un prestataire a pris votre demande",
    nl: "Een vakman heeft uw aanvraag aangenomen",
    en: "A provider has taken your request",
  },
  "statut.annulee": {
    fr: "Demande annulée",
    nl: "Aanvraag geannuleerd",
    en: "Request cancelled",
  },
  "statut.personne": {
    fr: "Personne n'a répondu",
    nl: "Niemand heeft geantwoord",
    en: "Nobody answered",
  },
  "statut.recherche": {
    fr: "Recherche d'un prestataire en cours…",
    nl: "Er wordt een vakman gezocht…",
    en: "Looking for a provider…",
  },
  "mission.acceptee": {
    fr: "Acceptée, le prestataire va partir",
    nl: "Aanvaard, de vakman vertrekt zo",
    en: "Accepted, the provider is about to leave",
  },
  "mission.en_route": {
    fr: "Le prestataire est en route",
    nl: "De vakman is onderweg",
    en: "The provider is on the way",
  },
  "mission.sur_place": {
    fr: "Le prestataire est arrivé",
    nl: "De vakman is aangekomen",
    en: "The provider has arrived",
  },
  "mission.validee": {
    fr: "Intervention validée",
    nl: "Opdracht gevalideerd",
    en: "Job validated",
  },
  "mission.terminee": {
    fr: "Intervention terminée",
    nl: "Opdracht afgerond",
    en: "Job finished",
  },
  "mission.annulee": {
    fr: "Intervention annulée",
    nl: "Opdracht geannuleerd",
    en: "Job cancelled",
  },

  // --- Statuts, côté prestataire ---
  "transition.partir": { fr: "Je pars", nl: "Ik vertrek", en: "I am leaving" },
  "transition.arrive": { fr: "Je suis arrivé", nl: "Ik ben aangekomen", en: "I have arrived" },
  "transition.termine": {
    fr: "L'intervention est terminée",
    nl: "De opdracht is afgerond",
    en: "The job is finished",
  },
  "pro.statut_acceptee": {
    fr: "Acceptée, pas encore commencée",
    nl: "Aanvaard, nog niet begonnen",
    en: "Accepted, not started yet",
  },
  "pro.statut_en_route": { fr: "En route", nl: "Onderweg", en: "On the way" },
  "pro.statut_sur_place": { fr: "Sur place", nl: "Ter plaatse", en: "On site" },
  "pro.statut_terminee": {
    fr: "Terminée, en attente de validation du demandeur",
    nl: "Afgerond, wacht op validatie door de aanvrager",
    en: "Finished, awaiting the requester's validation",
  },
  "pro.statut_validee": {
    fr: "Validée par le demandeur",
    nl: "Gevalideerd door de aanvrager",
    en: "Validated by the requester",
  },
  "pro.statut_annulee": { fr: "Annulée", nl: "Geannuleerd", en: "Cancelled" },
  "pro.devis_expire": {
    fr: "Expiré sans réponse",
    nl: "Vervallen zonder antwoord",
    en: "Expired without an answer",
  },
  "pro.devis_attente": {
    fr: "En attente de réponse",
    nl: "In afwachting van antwoord",
    en: "Awaiting an answer",
  },
  "pro.devis_accepte": { fr: "Accepté", nl: "Aanvaard", en: "Accepted" },
  "pro.devis_refuse": { fr: "Refusé", nl: "Geweigerd", en: "Declined" },

  // Les urgences en minuscule : elles apparaissent au milieu d'une phrase
  // (« Urgence : tout de suite »), là où les libellés du formulaire sont des
  // options de liste. Deux usages, deux clés — les confondre donnerait une
  // majuscule au milieu d'une phrase, ou l'inverse.
  "urgence.phrase_haute": { fr: "tout de suite", nl: "onmiddellijk", en: "right away" },
  "urgence.phrase_normale": { fr: "dans la journée", nl: "vandaag nog", en: "within the day" },
  "urgence.phrase_basse": { fr: "peut attendre", nl: "kan wachten", en: "can wait" },
} as const satisfies Record<string, Textes>;

export type CleTexte = keyof typeof TEXTES;

/**
 * Toutes les clés, pour que les tests puissent parcourir la table entière.
 *
 * **Exposée exprès.** Un test qui vérifie trois clés choisies à la main ne dit
 * rien des deux cents autres, et c'est précisément dans celles qu'on n'a pas
 * regardées qu'une traduction manque.
 */
export const CLES = Object.keys(TEXTES) as CleTexte[];

/**
 * Le texte d'une clé, dans la langue donnée.
 *
 * `remplacements` substitue les `{nom}` du gabarit. Une clé sans traduction ne
 * peut pas exister : le type l'interdit.
 */
export function t(
  locale: LocaleKlaar,
  cle: CleTexte,
  remplacements?: Record<string, string | number>,
): string {
  let texte: string = TEXTES[cle][locale];
  if (remplacements) {
    for (const [nom, valeur] of Object.entries(remplacements)) {
      texte = texte.replaceAll(`{${nom}}`, String(valeur));
    }
  }
  return texte;
}

/**
 * La langue choisie, ou `null` si l'on n'a jamais choisi.
 *
 * **Distinct de `localeAffichee()`**, qui rend toujours une langue : ici,
 * `null` veut dire « cette personne n'a rien demandé », et c'est ce qui permet
 * de suivre le navigateur au premier passage sans écraser un choix explicite au
 * second.
 */
export function langueChoisie(): LocaleKlaar | null {
  if (typeof localStorage === "undefined") return null;
  const brut = localStorage.getItem(CLE_LANGUE);
  return brut === "fr" || brut === "nl" || brut === "en" ? brut : null;
}

/**
 * Enregistre le choix et l'applique à la page.
 *
 * **`document.documentElement.lang` est mis à jour**, pas seulement une variable
 * : c'est lui que lisent `localeAffichee()`, les lecteurs d'écran et la
 * césure du navigateur. Un choix qui ne changerait qu'un état interne laisserait
 * une page annoncée en français lue à voix haute avec un accent français sur du
 * néerlandais.
 */
export function choisirLangue(locale: LocaleKlaar): void {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(CLE_LANGUE, locale);
  }
  appliquerLangue(locale);
}

/** Applique une langue au document, sans rien enregistrer. */
export function appliquerLangue(locale: LocaleKlaar): void {
  if (typeof document !== "undefined" && document.documentElement) {
    // Le suffixe régional est conservé pour le français et le néerlandais :
    // « fr-BE » et « nl-BE » ne se lisent pas comme « fr-FR » et « nl-NL », et
    // les formats de date et de monnaie en dépendent.
    document.documentElement.lang =
      locale === "en" ? "en" : `${locale}-BE`;
  }
}

/**
 * Rétablit le choix enregistré au chargement de la page.
 *
 * Rend la langue effective, pour que l'appelant n'ait pas à la relire.
 */
export function restaurerLangue(): LocaleKlaar {
  const choisie = langueChoisie();
  if (choisie) {
    appliquerLangue(choisie);
    return choisie;
  }
  // Aucun choix : la coquille reste dans sa langue déclarée.
  if (typeof document !== "undefined") {
    const brut = document.documentElement?.lang?.slice(0, 2);
    if (brut === "fr" || brut === "nl" || brut === "en") return brut;
  }
  return "fr";
}

/**
 * L'étiquette BCP 47 correspondant à une locale Klaar.
 *
 * **Le suffixe belge compte pour le formatage.** « fr-BE » écrit 1 234,50 et
 * « fr-FR » aussi, mais la monnaie et les dates diffèrent ; « nl-BE » et
 * « nl-NL » divergent franchement sur les nombres. L'anglais n'a pas de
 * variante belge : lui en inventer une donnerait des formats que personne
 * n'attend, donc il reste `en`.
 */
export function etiquetteBcp47(locale: LocaleKlaar): string {
  return locale === "en" ? "en" : `${locale}-BE`;
}
