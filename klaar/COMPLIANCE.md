# Conformité — à lire avant tout déploiement

Ce dépôt est publié sous licence MIT (voir `LICENSE.md` et `docs/adr/ADR-009-license-mit.md`).
La clause *AS IS* de MIT couvre la garantie logicielle. **Elle ne couvre pas la conformité
réglementaire de votre déploiement**, qui vous incombe entièrement.

Ce code met en œuvre des flux relevant de plusieurs régimes contraignants en Belgique et
dans l'Union. Le déployer tel quel, avec de vrais utilisateurs, sans le travail juridique
correspondant, vous met en infraction — pas l'auteur.

## Ce qui n'est pas fourni

| Obligation | Régime | État dans ce dépôt |
|---|---|---|
| **DPIA (analyse d'impact)** avant tout traitement de géolocalisation | RGPD art. 35 | **Absente.** Obligatoire *avant* le traitement, pas après. |
| Droit à l'effacement (art. 17) | RGPD | **Partiel.** Voir la section dédiée : ce qui existe est effacé, ce qui n'existe pas encore ne l'est pas. |
| Analyse de classification des travailleurs | Loi BE du 26/04/2024 + directive UE 2024/2831 (Platform Work) | Absente. Les invariants de non-fixation des prix sont décrits en conception, pas audités. |
| Agrément / passeport établissement de paiement, SCA | DSP2 | Absent. Le séquestre s'appuie sur Stripe Connect ; l'agrément reste celui de votre entité. |
| Documentation et audit de biais d'un matching algorithmique | AI Act art. 10-15 | Décrit en conception (FR-012, FR-056), non implémenté. |
| Mesures CyFun Basic (MFA ops, chiffrement at-rest, journal WORM) | NIS2 | Non implémentées. Les stories correspondantes sont bloquées faute de provisioning. |
| Régime TVA, taux applicables, facturation | TVA BE (21 % / 6 % / 12 %) | Décrit en conception, non implémenté. |

## Ce qui est fourni

Une **architecture** qui prend ces contraintes au sérieux dès la conception : séparation
hexagonale permettant d'isoler les traitements réglementés, journal d'audit prévu comme
immuable, minimisation des données pensée au niveau du modèle, traçabilité des exigences
vers les documents de conception (`docs/bmad-livrables/`).

C'est un point de départ défendable. Ce n'est pas une conformité.

## Un point connu, désormais corrigé

Le span racine de `tracing-actix-web` journalisait par défaut `http.client_ip` et
`http.user_agent`. Une adresse IP est une donnée personnelle, et l'agent utilisateur
contribue à l'empreinte du navigateur : les inscrire à chaque requête constituait un
traitement que rien ne documentait. Sans conséquence tant que `/api/v1/health` était le
seul endpoint, la question a cessé d'être théorique avec les endpoints d'abonnement push.

Corrigé par un constructeur de span dédié (`crates/klaar-api/src/telemetry.rs`), qui
conserve route, méthode, code et identifiant de requête, et laisse tomber les deux autres.
La route journalisée est le motif (`/missions/{id}`) et non le chemin brut, pour la même
raison : un identifiant de Mission dans chaque ligne de journal reconstitue l'activité
d'une personne.

Une première tentative se contentait de déclarer ces champs vides et **ne marchait pas** —
la macro `root_span!` les renseigne elle-même, et les journaux contenaient toujours l'IP.
Le test `crates/klaar-api/tests/telemetry.rs` inspecte les journaux réellement émis,
précisément pour que cette illusion ne puisse pas se reproduire sans être vue.

## Écarts assumés avec le PRD (Story 1.1, FR-001)

Trois points où le code ne suit pas la lettre de FR-001. Chacun est un arbitrage, pas un
oubli.

**1. Pas de `409 EMAIL_ALREADY_EXISTS`.** FR-001 le demande dans son scénario `@negative`,
et exige deux paragraphes plus bas une réponse « identique (timing + payload) » que
l'adresse existe ou non. Les deux ne peuvent pas être vrais : un `409` fait de
l'inscription un moyen de tester la présence de n'importe quelle adresse. L'inscription
répond donc toujours `202 SIGNUP_ACCEPTED`. L'indistinguabilité n'est pas seulement dans le
corps : le mot de passe est haché avant que la base soit interrogée, et un courriel part
dans les deux cas.

**2. Un courriel part même quand l'adresse est déjà prise**, alors que FR-001 écrit
« aucun email n'est envoyé ». C'est la conséquence du point précédent : sans envoi, le
chemin « adresse déjà prise » est plus court d'un appel réseau et se reconnaît au
chronomètre. Le message informe le titulaire qu'une inscription a été tentée et **ne
contient aucun lien** — sinon s'inscrire avec l'adresse d'autrui deviendrait un moyen de
lui expédier un jeton.

**3. Le jeton de vérification n'est pas un JWT.** FR-001 dit « token JWT courte durée
(1 h) » et exige aussi qu'il soit marqué utilisé, donc non rejouable. Un JWT est vérifiable
sans état côté serveur, ce qui interdit précisément de le marquer. Tenir les deux imposerait
une table de jetons consommés, c'est-à-dire l'état que le JWT prétendait éviter. Le code
emploie un jeton opaque de 32 octets, conservé haché (SHA-256) et à usage unique : même
coût en base, sans la surface d'attaque d'un JWT.

**Non fourni :** le challenge hCaptcha après trois échecs, décrit par le scénario
`@security` de FR-001. Il suppose un compte chez un tiers et un appel sortant vers lui,
hors du périmètre vitrine. La limitation de débit reste la seule borne d'abus.

## Écart assumé avec le PRD (Story 1.2, FR-001)

Le tableau des endpoints du PRD annonce `GET /api/v1/auth/verify-email?token=…`. Le code
sert `POST /api/v1/auth/verify-email`. Les passerelles de messagerie d'entreprise visitent
les liens des courriels avant leur destinataire pour les analyser : un `GET` qui consomme
le jeton est consommé par l'antivirus, et l'utilisateur trouve un lien déjà utilisé au
moment où il clique. Le lien pointe donc une page statique de la PWA, qui présente ensuite
le jeton par un `POST` — qu'un analyseur de liens n'exécute pas.

Le jeton est conservé haché (SHA-256), marqué consommé dans la même transaction que
l'activation du compte, et la ligne est verrouillée (`FOR UPDATE`) le temps de l'opération :
deux clics simultanés n'activent qu'une fois.

## Sessions : ce qui est fourni et ce qui manque (Story 1.3, FR-004)

Fourni : jeton d'accès JWT HS256 d'une heure, refresh opaque de 30 jours conservé haché,
cookie `HttpOnly` `Secure` `SameSite=Lax` restreint au chemin `/api/v1/auth`, algorithme de
vérification fixé explicitement (un jeton annonçant `alg: none` est refusé, un test le
vérifie).

Fourni depuis la Story 1.4 : rotation à chaque usage, détection de rejeu, coupure de la
famille entière au premier jeton rejoué, et déconnexion explicite. Un refresh volé n'est
donc plus utilisable trente jours : il l'est jusqu'à la prochaine rotation du porteur
légitime, après quoi la chaîne est coupée pour les deux.

**Le *binding* reste partiel.** Le scénario `@security` de FR-004 demande un lien
« UA + IP + device ». L'agent utilisateur est lié, sous forme d'empreinte SHA-256, et un
changement lève `SESSION_CONTEXT_CHANGED` dans le journal d'audit **sans couper la
session** : les navigateurs changent d'agent à chaque mise à jour, bloquer là-dessus
déconnecterait tous les utilisateurs toutes les quelques semaines sans qu'aucun vol n'ait
eu lieu. L'adresse IP n'est **pas** liée : un téléphone en change plusieurs fois par trajet
en passant du wifi aux données mobiles. Le challenge itsme prévu en réponse à l'anomalie
n'est pas fourni — il suppose un contrat itsme, hors périmètre. L'anomalie est donc
consignée, sans remédiation automatique.

**RGPD.** L'agent utilisateur n'est pas conservé, seulement son empreinte, et à cette seule
fin de détection. C'est une mesure de sécurité au sens de l'art. 32, pas une mesure
d'analyse d'audience.

`KLAAR_JWT_SECRET` est obligatoire au démarrage : sans elle, `klaar-api` refuse de démarrer
plutôt que d'en générer une, ce qui invaliderait toutes les sessions à chaque redémarrage.
HS256 signifie que le secret sert à la fois à signer et à vérifier : ne le partagez pas avec
un second service, ce serait lui donner le pouvoir d'émettre des jetons.

## Annulation : ce que le motif a le droit d'être (Story 3.5, FR-014)

FR-014 veut le motif d'annulation « stocké pour analytics ». Le motif est un
**vocabulaire fermé de cinq codes**, pas un texte libre : ce dernier inviterait à
écrire une donnée personnelle non sollicitée dans un champ dont la finalité
annoncée est statistique. Un motif hors vocabulaire est refusé, pas ramené sur
`OTHER`.

**Le motif vit sur la Demande, pas dans le journal d'audit.** Il disparaît donc
avec elle quand le compte est effacé (art. 17), sans qu'aucune procédure de purge
n'ait à s'en souvenir ; dans le journal, il survivrait à l'effacement. Une
contrainte de base impose qu'un motif n'existe que sur une Demande annulée.

**L'avis envoyé aux prestataires notifiés ne dit pas pourquoi.** Le motif
appartient au demandeur ; le diffuser à dix entreprises en ferait un jugement.

**Écart au FR, assumé : 404 et non 403** pour la Demande d'autrui. Distinguer
« elle n'existe pas » de « elle n'est pas à vous » laisserait apprendre quelles
Demandes existent. C'est la précédence de l'anti-énumération, déjà retenue
ailleurs sur ce projet.

**Après attribution, l'annulation est refusée** : le prestataire est peut-être
déjà en route, et c'est la Mission qu'il faut alors annuler (FR-023). La course
entre une annulation et une acceptation est tranchée par PostgreSQL, les deux
écritures portant sur la même ligne.

## Fin de tour et élargissement (Story 3.6, FR-015)

**Contradiction du PRD tranchée.** FR-013 refusait une acceptation après cinq
minutes, FR-015 annonce `NO_MATCH` après trente secondes. Trente secondes
l'emportent : cette règle rejette aussi tout ce que la règle à cinq minutes
rejetait, donc elle satisfait les deux ; l'inverse est faux.

**Trois élargissements au maximum, puis annulation.** L'échelle s'arrête à vingt
kilomètres parce que, depuis n'importe quel point de la Région de
Bruxelles-Capitale, vingt kilomètres la couvrent entièrement. Le quatrième essai
annule la Demande plutôt que de la laisser en attente : entretenir l'idée que
quelque chose peut encore arriver serait pire que de le dire.

**Le compteur d'élargissements ne se remet jamais à zéro**, et la relance est un
compare-and-swap sur ce compteur : deux clics sur « élargir » ne consomment
qu'une des trois chances du demandeur.

**Aucun demandeur n'est notifié deux fois.** Le balayage sélectionne et éteint en
une seule instruction, avec `FOR UPDATE SKIP LOCKED` : deux passages concurrents
se partagent le travail sans jamais rendre la même Demande.

**Le score se normalise sur le rayon du tour.** Le paramètre ajouté à `calculer`
est un paramètre du tour, identique pour tous les candidats d'un même tour : il
ne peut en distinguer aucun, et la garantie de FR-012 tient toujours. Un test
fixe ce raisonnement, et le test de signature échouera de nouveau au prochain
ajout.

**Limite assumée : l'avis de fin de tour part en français** quelle que soit la
langue du compte. Lire la langue du demandeur demanderait un dépôt de plus au
binaire de balayage pour un message de deux lignes.

## Acceptation : ce que la course garantit, et ce qu'elle ne garantit pas (Story 3.4, FR-013)

Cinq prestataires notifiés peuvent accepter la même Demande dans la même
seconde. Un seul l'obtient, et la garantie n'est pas dans le code applicatif :
elle est dans un `UPDATE … WHERE statut = 'BROADCASTING' RETURNING …` que
PostgreSQL sérialise. Le passage de la Demande en `MATCHED` et la création de la
Mission forment une seule transaction.

La règle « une Mission à la fois » repose sur un index unique partiel et non sur
un contrôle applicatif : vérifier puis insérer laisserait passer deux
acceptations simultanées.

**L'éligibilité est revérifiée au moment d'accepter**, pas au matching : un
prestataire suspendu entre la notification et le geste ne passe pas. Le secteur
l'est aussi, ce que FR-013 ne demandait pas — sans quoi un prestataire d'un
autre métier pouvait prendre une Demande dont il connaissait l'identifiant.

**Les refus d'éligibilité ne renseignent pas.** Un compte non prestataire ou
suspendu reçoit le même `PROVIDER_NOT_ELIGIBLE` que la Demande existe, soit déjà
prise, ou n'ait jamais existé.

**Limite assumée : aucune tâche de fond n'éteint les Demandes expirées.** Une
Demande de plus de cinq minutes reste `BROADCASTING` en base ; l'expiration se
constate à la lecture, au moment où quelqu'un tente de l'accepter. Le statut
stocké ne suffit donc pas à lui seul à dire si une Demande est vivante, et toute
requête d'exploitation qui l'ignorerait compterait des Demandes mortes comme
actives.

## Notifications : ce qu'un écran verrouillé affiche (Story 3.3)

Une notification push s'affiche sur un écran verrouillé, lisible par quiconque passe à côté
du téléphone. Le message envoyé aux prestataires ne porte donc **ni la description du
problème, ni l'adresse, ni rien du demandeur** : seulement le code de secteur, la distance
arrondie à la centaine de mètres et l'urgence.

Le chiffrement de la charge (RFC 8291) protège le transit, pas l'affichage : les deux
problèmes sont distincts, et seul le second se règle en choisissant ce qu'on écrit.

La distance est arrondie parce qu'au mètre près, croisée avec la position connue du
prestataire, elle situerait le demandeur à son domicile.

Un abonnement déclaré disparu par le service de push est supprimé, conformément au principe
de limitation de conservation : un abonnement mort est une donnée personnelle sans finalité.

## Matching : ce que le score voit, et ce qu'il ne voit pas (Story 3.2, FR-012)

L'AI Act exige qu'une décision automatisée puisse s'expliquer et qu'aucun attribut protégé
ne la biaise. La garantie n'est pas une promesse : la fonction de score **ne reçoit que trois
nombres** — une distance, une ancienneté de contrôle, une note éventuelle. Elle ne peut pas
voir un nom, une adresse, une langue ou une photo, parce qu'on ne les lui donne pas.

La table `trace_matching` conserve, pour chaque Demande, tous les candidats examinés — retenus
comme écartés — avec leur score, sa ventilation par critère et le motif de l'écart. Elle
répond à « pourquoi n'ai-je pas été notifié ? », qu'un prestataire est en droit de poser. Elle
est écrite **avant** que les candidats ne soient rendus : une notification qu'aucune trace
n'explique est précisément ce que l'AI Act interdit.

**Ce qui manque au score** : le rating, que FR-012 nomme comme critère. Le bounded context
Trust n'existe pas. Il est traité comme absent et son poids redistribué, faute de quoi tout
nouveau prestataire serait classé derrière un prestataire mal noté. L'absence figure dans la
ventilation conservée.

**Non fourni** : l'audit de biais semestriel (Story 3.8), et le second tour à rayon élargi
(Story 3.6). Le matching est par ailleurs lancé dans la requête et non par une file de
travaux, faute d'infrastructure de file dans ce périmètre.

## Prestataires : le KYC n'est pas fait (Story 1.6, FR-003)

FR-003 exige la validation du numéro à la Banque-Carrefour des Entreprises, le contrôle de
l'état de faillite et la collecte d'une attestation d'assurance. **Rien de cela n'est
fourni** : l'API de la BCE, le stockage objet chiffré et l'antivirus sont hors du périmètre.

Ce qui est fourni : la **clé de contrôle** du numéro BCE, vérifiée hors ligne. Elle attrape
une faute de frappe ou un numéro inventé, jamais l'existence réelle d'une entreprise. Un
numéro bien formé n'est pas un numéro valide.

Ce qui remplace le contrôle : un prestataire naît `PENDING_KYC` et n'en sort que sur
présentation d'une `PreuveKyc`, dont la seule fabrique utilisable aujourd'hui s'appelle
`demonstration`. L'origine est écrite en base, et une contrainte interdit qu'un prestataire
actif n'en porte aucune. **Conséquence à connaître** : toute fiche prestataire de ce
déploiement porte `origine_kyc = 'DEMONSTRATION'`, et se retrouve par
`SELECT ... WHERE origine_kyc = 'DEMONSTRATION'`.

Le peuplement passe par le binaire `klaar-prestataires-demo`, qui refuse de tourner sans
`KLAAR_PRESTATAIRES_DEMO=1`. Ce drapeau ne protège de rien — qui peut lancer le binaire peut
poser la variable — mais il empêche qu'une exécution distraite peuple une base réelle.

## Périmètre géographique : un rectangle, pas la Région (Story 3.1, FR-011)

Le contrôle `GEO_OUTSIDE_RBC` ramène les dix-neuf communes de la Région de
Bruxelles-Capitale à un **rectangle englobant**. Il sur-accepte : des points du Brabant
flamand tout proches de la frontière régionale — Kraainem, Drogenbos — y tombent.

Le choix est délibéré. Sur-accepter fait entrer quelques Demandes hors périmètre, qu'un
prestataire refusera ; sous-accepter refuserait des Bruxellois chez eux. Un test constate
cette sur-acceptation plutôt que de la masquer.

**À remplacer avant toute mise en service** par le contour réel, qui viendra des données
OpenStreetMap de la Story 0.11 — aujourd'hui bloquée faute d'hébergement pour le
tile-server.

## Demandes : ce qui n'est pas contrôlé (Story 3.1, FR-011)

**La méthode de paiement n'est pas exigée** dans le déploiement vitrine
(`KLAAR_EXIGER_METHODE_PAIEMENT=0`), faute de compte Stripe (Story 1.7). Le contrôle existe,
avec son port et son `422`, et il est **actif par défaut** : le désactiver est un geste
explicite, journalisé au démarrage. Conséquence à connaître : une Demande peut être créée
sans qu'aucun moyen de paiement ne la garantisse.

**Le matching n'est pas déclenché.** Une Demande est créée en `BROADCASTING` et y reste :
la recherche de prestataires et leur notification appartiennent aux Stories 3.2 et 3.3.
L'interface le dit à l'utilisateur plutôt que de laisser croire qu'un dépanneur est en route.

**Les photos ne sont pas prises en charge** : leur stockage chiffré demande un compartiment
objet provisionné, hors périmètre.

## Prix indicatifs : ce que la fourchette expose (Story 2.3, FR-009)

Une fourchette est faite de deux prix réels : son minimum et son maximum **sont** des
montants qu'un prestataire a effectivement facturés. Un seuil de cinq Missions conditionne
donc sa publication, conformément à l'exemple de FR-009.

**Ce que ce seuil ne supprime pas** : à cinq Missions, la fourchette publie deux prix
facturés sur cinq. Le seuil retenu est celui du PRD ; le relever demanderait un jeu de
données réel à observer, qui n'existe pas encore. À réévaluer avant toute mise en service
avec de vrais prestataires — c'est le genre de réglage qu'un chiffre choisi sur le papier
ne tranche pas.

Le seuil porte sur l'échantillon d'entrée, pas sur ce qu'il reste après exclusion des
valeurs aberrantes : l'appliquer aux deux contredirait l'exemple du FR. La contrainte est
reposée par la base (`nb_missions >= 5`), pour qu'aucun chemin d'écriture ne la contourne.

**Non fourni** : le job de calcul. Il lit l'historique des Missions, qui n'existe pas avant
l'Epic 3. La table d'agrégat reste vide et toutes les fourchettes sont absentes, ce qui est
l'état prévu par FR-009 `@negative` au lancement.

## Droit à l'effacement (Story 1.9, FR-005, RGPD art. 17)

`POST /api/v1/me/erase` avec la confirmation `DELETE` programme l'effacement à trente jours ;
`POST /api/v1/me/erase/cancel` l'annule ; le binaire `klaar-effacer`, à planifier, exécute
les échéances.

**Ce qui est effacé** : adresse, empreinte du mot de passe, jetons de vérification,
sessions de rafraîchissement, abonnements push. La ligne de compte est **vidée, pas
supprimée** — la supprimer emporterait par cascade les entrées du journal d'audit, que le
scénario `@security` de FR-005 exige de conserver. L'adresse est remplacée par une valeur
dérivée de l'identifiant sur le domaine `.invalid`, réservé par la RFC 2606.

**Ce qui n'est pas effacé, faute d'exister** : Missions, factures, traces de
géolocalisation, identifiants Stripe. Leurs bounded contexts arrivent aux Epics 3 et
suivants ; l'effacement devra les traiter à ce moment-là, et **ne le fait pas aujourd'hui**.
Les refus « Mission en cours » et « dette paiement » que décrit FR-005 sont hors d'atteinte
pour la même raison.

**Ajout non demandé par FR-005, et qui en découle** : l'annulation pendant le délai de
grâce. Trente jours n'ont de raison d'être que s'ils sont réversibles. Le compte reste donc
utilisable pendant l'attente, sans quoi son titulaire ne pourrait pas se connecter pour
annuler sa propre demande.

**Non fourni** : la notification des sous-traitants et des destinataires (art. 19), et
l'export des données (art. 20, portabilité), qui est un droit distinct.

## Verrouillage anti-brute-force (Story 1.8, FR-007)

Cinq échecs dans une fenêtre glissante de dix minutes verrouillent le compte quinze
minutes, avec une entrée d'audit `ACCOUNT_LOCKED` et une alerte au titulaire.

**Écart avec FR-007** : le `423 ACCOUNT_LOCKED` n'est renvoyé qu'à un appelant ayant donné
le bon mot de passe. Le FR le demande « correct ou non », mais exige au scénario suivant
qu'aucune information ne fuite sur l'existence du compte — or un `423` sur une adresse au
hasard révèle qu'elle a un compte. Un mauvais mot de passe sur un compte verrouillé rend la
même réponse qu'une adresse inconnue, avec le même temps de traitement.

**Le verrou est aussi une arme retournable** : un tiers peut fermer le compte d'autrui en
échouant cinq fois sur son adresse. Trois choix limitent la portée — durée courte et
réouverture automatique, fenêtre glissante qui ne compte pas cinq oublis étalés, et le
verrou en cours qui n'est jamais prolongé par les tentatives suivantes. La limitation par
adresse IP reste la première ligne : depuis une source unique, le verrou n'est pas
atteignable.

**Non fourni** : le déverrouillage manuel par le support, et toute mesure d'escalade
au-delà de quinze minutes.

## Limites connues de la limitation de débit

Le compteur des cinq inscriptions par heure et par adresse vit **en mémoire du processus**
(`crates/klaar-api/src/limitation.rs`). Il tient pour un déploiement à un seul exemplaire et
jusqu'au redémarrage ; derrière plusieurs instances, chacune compterait pour elle et la
limite effective serait multipliée d'autant. Une version partagée (Redis ou table dédiée)
viendra avec le déploiement réel, aujourd'hui bloqué faute de provisioning.

L'adresse IP n'est pas conservée : la clé est son empreinte SHA-256 tronquée. `X-Forwarded-For`
n'est cru que si `KLAAR_DERRIERE_PROXY=1` est posé, faute de quoi n'importe qui contournerait
la limite en changeant un en-tête.

## Ce que le journal d'audit ne contient pas

Les entrées `USER_SIGNUP` et `USER_SIGNUP_DUPLICATE` portent un code, un horodatage et,
quand c'est légitime, l'identifiant du compte. **Ni adresse IP, ni agent utilisateur** — même
raisonnement que pour les journaux applicatifs : ces données sont personnelles, et aucune
finalité ni durée de conservation n'a été établie pour elles ici. La limite est réelle : un
audit de sécurité complet voudra l'origine des tentatives.

Une tentative sur une adresse déjà prise est consignée **sans** l'identifiant du titulaire.
Autrement, le journal d'audit deviendrait l'oracle d'énumération que la réponse HTTP refuse
d'être.

## Vulnérabilité transitive acceptée

`cargo audit` et `cargo deny` ignorent **RUSTSEC-2026-0258** (h2 < 0.4.16, déni de service
par frames DATA vides), transitive via `actix-http` — toute la branche h2 0.3.x d'actix-web
v4 en hérite et aucun correctif amont n'existe à ce jour. Acceptable tant que le service
n'est pas exposé publiquement. **À réévaluer à chaque mise à jour de dépendances**
(`cargo tree -i h2`).

## Publication de ce dépôt

Si vous forkez ou republiez, notez que la version d'origine a été extraite d'un dépôt privé
contenant des documents commerciaux et les besoins d'un prospect. Ces documents ne font pas
partie de la publication et ne doivent pas y être réintroduits, historique git compris.
