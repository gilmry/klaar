/**
 * Navigation principale : quelles destinations, dans quel ordre, selon l'état
 * de session.
 *
 * **Pourquoi un module à part et non du balisage dans la coquille.** Une
 * navigation écrite en dur dans `AppLayout.astro` ne se teste qu'en lançant un
 * navigateur ; ici, la liste des liens est une fonction pure, et un test
 * unitaire peut affirmer qu'aucune entrée ne pointe vers une page qui n'existe
 * pas, ce qu'aucune relecture ne garantit durablement.
 *
 * **Ce que le front sait de l'utilisateur, et ce qu'il ne sait pas.** Le jeton
 * d'accès ne porte que `sub`, `iat` et `exp` (voir `klaar-api/src/jwt.rs`,
 * dont un test vérifie qu'aucune autre revendication ne voyage). Il n'y a donc
 * **aucun rôle lisible côté client** : la navigation distingue « connecté » de
 * « pas connecté », et rien de plus fin. Savoir si un compte est prestataire
 * demanderait d'appeler `GET /api/v1/providers/me/availability` — qui répond
 * 403 pour un non-prestataire — à chaque page, pour dessiner un menu. Ce n'est
 * pas fait : une requête réseau par affichage de coquille est un coût réel, et
 * un 403 attendu dans les journaux d'accès est un bruit qui masque les vrais.
 * La conséquence est écrite plutôt que masquée : « Je suis prestataire » est
 * proposé à tout le monde, et c'est la page `/prestataire` qui refuse.
 *
 * **`/ops` n'est pas ici.** La console d'exploitation est atteignable depuis le
 * pied de page, pas depuis la navigation principale — voir `LIEN_OPS`.
 */
import type { CleTexte } from "./i18n";

export interface LienNavigation {
  /** Chemin absolu d'une page réellement présente dans `src/pages/`. */
  href: string;
  /** Clé i18n du libellé. Jamais de texte en dur : la coquille est trilingue. */
  cle: CleTexte;
}

/**
 * Les liens de la navigation principale.
 *
 * L'ordre suit ce que quelqu'un vient faire, pas l'organisation du code :
 * demander un dépannage d'abord, parce que c'est la raison d'être du service ;
 * le compte ensuite, parce qu'on ne s'en occupe qu'une fois.
 */
export function liensPrincipaux(connecte: boolean): LienNavigation[] {
  const liens: LienNavigation[] = [
    { href: "/", cle: "nav.accueil" },
    { href: "/demande", cle: "nav.demande" },
    { href: "/catalogue", cle: "nav.catalogue" },
  ];

  if (connecte) {
    liens.push({ href: "/mon-compte", cle: "nav.compte" });
    liens.push({ href: "/prestataire", cle: "nav.prestataire" });
  } else {
    liens.push({ href: "/inscription", cle: "nav.inscription" });
    liens.push({ href: "/connexion", cle: "commun.me_connecter" });
    // Un prestataire qui arrive sans session doit pouvoir trouver son espace :
    // c'est la page elle-même qui lui demandera de se connecter.
    liens.push({ href: "/prestataire", cle: "nav.prestataire_visiteur" });
  }

  return liens;
}

/**
 * Le lien vers la console d'exploitation, servi dans le pied de page.
 *
 * **Ni caché, ni mis en avant.** Le cacher entièrement — l'état d'avant ce
 * commit, où aucun lien du site ne menait à `/ops` — oblige l'équipe
 * d'exploitation à retenir une URL, ce qui finit en URL partagée par écrit. Le
 * mettre dans la navigation principale en fait une rubrique du site pour un
 * visiteur, ce qu'il n'est pas.
 *
 * **Ce n'est pas une mesure de sécurité, et ce n'est pas censé en être une.**
 * La page `/ops` est une coquille statique : elle n'affiche rien avant un
 * `POST /api/v1/ops/login` réussi (mot de passe **et** code TOTP), et chaque
 * route `/api/v1/ops/*` revérifie le jeton d'exploitation côté serveur. Rendre
 * la page introuvable n'ajouterait donc aucune garantie ; c'est la
 * vérification serveur qui en donne une.
 */
export const LIEN_OPS: LienNavigation = { href: "/ops", cle: "nav.ops" };

/**
 * Toutes les pages que la navigation doit rendre atteignables.
 *
 * Sert de garde-fou aux tests : une page ajoutée dans `src/pages/` sans lien
 * entrant est un écran que personne ne trouvera. Les deux exceptions sont des
 * pages de destination, atteintes autrement que par un clic :
 *
 * - `hors-ligne` est servie par le service worker quand le réseau manque ;
 * - `verifier-email` est ouverte depuis le lien reçu par courriel, et porte le
 *   jeton de vérification dans son URL.
 */
export const PAGES_SANS_LIEN_ENTRANT = ["hors-ligne", "verifier-email"] as const;
