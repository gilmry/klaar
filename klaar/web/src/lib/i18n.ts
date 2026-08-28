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
} as const satisfies Record<string, Textes>;

export type CleTexte = keyof typeof TEXTES;

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
