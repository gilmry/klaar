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

## File d'attente hors ligne (Story 3.9)

Une Demande écrite sans réseau est **conservée sur l'appareil**, dans IndexedDB,
et part au retour de la connexion. L'écran distingue « en file » de « créée » :
rien n'a été envoyé au service, et le dire autrement ferait croire que des
prestataires ont été prévenus.

**Le rejeu reprend la session** depuis le cookie de rafraîchissement, le jeton
d'accès ne survivant pas au rechargement. Si la session ne peut pas être reprise,
l'écriture est refusée plutôt que rejouée : agir au nom de quelqu'un dont la
session a expiré serait pire que de perdre l'écriture.

**Limite assumée** : le service ne lit pas encore l'en-tête `Idempotency-Key`.
Ce qui protège d'une double soumission est la fenêtre de doublon de cinq minutes
(FR-011), qui rend la Demande existante au lieu d'en créer une seconde.

## Parcours filmés : ce qui est publié (Story 4.11)

Les vidéos publiées sur GitHub Pages sont l'enregistrement de tests joués contre
le service réel. Ce qu'elles montrent est donc l'état véritable du service, pas
une maquette.

**Les données filmées sont fictives.** Les comptes sont sur
`demo.klaar.invalid` — domaine réservé par la RFC 2606, où rien ne peut être
livré et qu'aucun compte réel ne peut porter. Les numéros d'entreprise sont
construits, jamais copiés d'une société existante. Une barrière de CI refuse la
publication si une adresse de messagerie grand public apparaît dans la vitrine.

**Deux quotas sont relevés pour le déploiement de démonstration** : la
limitation d'écritures sensibles par adresse et le quota horaire de Demandes par
compte. Ce sont des **chiffres** paramétrés, annoncés au démarrage, et non des
interrupteurs : un quota qu'on peut éteindre finit éteint en production.

**Un enregistrement absent est annoncé sur la page publiée.** Montrer cinq
vidéos sur six sans le dire laisserait croire qu'il n'y en a jamais eu que cinq.

**Écart avec un usage réel, assumé** : la géolocalisation est accordée au
contexte du navigateur, aucune boîte de dialogue système n'est cliquée.

## Ce que chacun voit d'une Demande (Story 4.10)

**Le prestataire ne voit pas l'adresse avant d'avoir pris l'intervention.** Avant
d'accepter, il dispose du secteur, de la description, de l'urgence et d'une
distance. L'adresse n'apparaît qu'une fois la Mission à lui, parce qu'il doit s'y
rendre. Donner à dix entreprises l'adresse d'un foyer pour un dépannage que neuf
ne feront pas n'a aucune justification.

Ce n'est pas une consigne : la structure qui porte la vue du prestataire n'a pas
de champ de position, et un test vérifie que la réponse HTTP n'en porte aucune
trace.

**Le demandeur apprend le nom de l'entreprise** dès l'attribution — savoir qui va
sonner à sa porte est le minimum. Rien d'autre du prestataire ne lui est exposé.

**Limite assumée : le suivi est un sondage** toutes les cinq secondes, arrêté dès
que la Demande est close. Le temps réel appartient au WebSocket de FR-018, non
livré.

## Webhooks de paiement : l'endpoint public qui écrit (Story 5.5, FR-028)

**C'est le seul endpoint sans authentification qui produit une écriture**, et
c'est délibéré : Stripe appelle depuis des adresses qui changent, sans jeton à
présenter. La signature HMAC-SHA256 tient lieu d'authentification, et elle est
donc le seul rempart entre un inconnu et une écriture sur l'argent de quelqu'un.

**Elle est vérifiée avant que la charge ne soit analysée.** Décoder le JSON d'un
appelant non authentifié lui offrirait une surface d'attaque gratuite. Et la
fenêtre de tolérance est contrôlée avant le calcul HMAC : faire le travail
cryptographique pour un événement qu'on refusera de toute façon laisserait un
envoi massif coûter plus qu'il ne devrait.

**La charge signée inclut l'horodatage**, ce qui ferme le rejeu : une requête
interceptée et renvoyée plus tard est refusée même si sa signature est
authentique. La fenêtre est de cinq minutes — plus large, le rejeu s'ouvre ;
plus étroite, un décalage d'horloge ferait perdre des événements réels.

**La comparaison est en temps constant, et porte sur toutes les signatures
présentées.** Sortir à la première différence d'octet révélerait le préfixe
commun par le temps de réponse.

**Un seul code de refus pour quatre causes.** En-tête illisible, schéma absent,
horodatage périmé, signature fausse : la réponse est identique. Distinguer
« périmé » de « faux » apprendrait à qui essaie qu'il a trouvé le secret.

**L'ancien schéma `v0` est refusé.** Tolérer un schéma déprécié laisse ouvert le
chemin qu'un attaquant choisira. Plusieurs `v1`, en revanche, sont acceptées :
c'est ce qui permet de faire tourner le secret sans perdre d'événement.

**Un secret absent ferme l'endpoint.** Sans lui, tout appel est refusé — y
compris ce qui serait authentique. En conclure qu'on peut tout accepter
« puisqu'il n'y a rien à vérifier » ferait d'une variable d'environnement
oubliée une porte ouverte.

**Le journal des événements est en insertion seule.** Remettre un événement à
« non appliqué » permettrait de rejouer une capture en effaçant sa trace,
c'est-à-dire de contourner exactement ce que la table protège. L'idempotence
elle-même est tenue par la clé primaire et non par une lecture préalable : deux
réceptions simultanées du même événement s'écraseraient sur un « lire puis
décider », et le prélèvement aurait lieu deux fois.

**Rien n'est journalisé d'une charge non authentifiée** : ni le corps, ni
l'en-tête, qui porte une signature. Seul le code de refus l'est.

## Ce que le séquestre garantit sans passerelle (Story 5.2, FR-024 à FR-027)

**L'argent n'est pas là, mais ses règles le sont.** Trois interdits que nulle
passerelle ne tient à notre place : capturer plus qu'autorisé et rembourser plus
que capturé créeraient de l'argent ; rembourser après versement le prendrait à
quelqu'un qui l'a déjà reçu. Un test parcourt toute l'échelle des
remboursements et vérifie l'égalité comptable à chaque pas.

**Le solde est calculé et jamais conservé** : un champ dérivé de ses composantes
devient faux à la première écriture oubliée, et c'est la façon habituelle dont
un compte se met à mentir.

**L'ordre des contrôles est lui-même une garantie** : un remboursement excessif
après versement doit dire « le prestataire a été payé », parce que la suite à
donner n'est pas celle d'un montant trop élevé.

## Médiation et contrôle d'entreprise (Stories 7.4 et 8.1, FR-036, FR-038)

**Une décision de médiation est définitive, et la base le grave.** Le
déclencheur refuse de retrancher un litige clos, même en SQL direct. Rouvrir
permettrait de revenir sur un remboursement déjà annoncé, et viderait la
première décision de sa valeur pour celui qu'elle a débouté. Le recours
au-delà est judiciaire, et l'écran le dit avant que le geste soit posé.

**Deux médiateurs sur le même dossier ne produisent qu'une décision.** Le
compare-and-swap sur le statut ferme la course. Lire puis écrire laisserait les
deux passer, et un second remboursement partirait sans trace de délibération.

**Aucun mouvement d'argent n'est exécuté**, et l'API le rend explicitement
(`execute: false`). Le séquestre est chez Stripe, non provisionné : l'écran
écrit « montants à verser », jamais « remboursé ». Annoncer un virement qui ne
vient pas transforme un litige tranché en second litige.

**La règle des quatre yeux porte sur le refus, pas sur la validation** (FR-038).
Une validation trop généreuse se corrige par une suspension au premier
incident ; un refus injuste ne se corrige pas, l'entreprise est déjà partie.
Exiger deux examinateurs pour valider doublerait le délai d'entrée de chaque
entreprise honnête pour se prémunir d'un risque déjà couvert.

**Un refus proposé ne produit aucun effet** tant qu'un autre compte n'a pas
confirmé, et la contrainte de base refuse qu'on confirme le sien : ce ne serait
pas une seconde paire d'yeux, ce serait un second clic.

**Un refus sans motif n'existe pas** : vingt caractères au moins, parce qu'une
entreprise refusée doit pouvoir savoir ce qu'on lui reproche, sans quoi elle ne
peut ni corriger ni contester. Un motif passé avec une **validation** est refusé
et non ignoré — l'ignorer laisserait son auteur croire qu'il a été consigné.

**Refusée, suspendue et retirée sont trois états distincts.** Un suspendu a été
actif ; un refusé n'est jamais entré ; une entreprise retirée n'a été jugée par
personne. Les confondre ferait apparaître dans les statistiques de sanction des
entreprises qui n'ont jamais travaillé, ou inscrirait au dossier de quelqu'un
qui s'est ravisé une décision que personne n'a prise.

**L'origine du contrôle dit ce qu'elle vaut.** `OPS_REVIEW` signifie qu'un
humain a lu les pièces — ni la BCE, ni rien. Le jour où l'adaptateur BCE
existera, les dossiers validés à la main resteront distinguables.

**Chaque consultation et chaque refus d'accès sont journalisés**, y compris la
simple lecture d'une file de dossiers : savoir qui regarde, et pas seulement qui
décide, est ce qu'un audit vient chercher.

## Session d'exploitation (Story 8.3, FR-040, FR-041)

**Les identifiants d'exploitation ne circulent plus qu'une fois.** La première
version de la console les reprenait à chaque requête en **paramètres d'URL** :
un mot de passe et un code TOTP dans la barre d'adresse, l'historique du
navigateur, l'en-tête `Referer` et les journaux d'accès du serveur. Le
`POST /ops/login` les prend désormais dans un corps de requête et rend un jeton
porteur ; l'ancienne forme est **retirée**, et un test vérifie qu'elle ne donne
plus rien. Une forme dépréciée mais encore acceptée resterait le chemin qu'un
outil ancien continuerait d'emprunter.

**Trente minutes, sans prolongation.** Une session d'exploitation ouvre des
dossiers nominatifs et des décisions sur l'argent d'autrui ; celle qui se
renouvelle à chaque clic finit ouverte toute la journée sur un poste partagé.

**La révocation d'un compte ferme ses sessions immédiatement.** La condition
`compte_ops.actif` est dans la requête qui lit la session, pas dans un balayage
périodique : un compte désactivé perd ses accès à la requête suivante, pas à
l'expiration de son jeton.

**Le jeton n'est conservé nulle part.** Côté serveur, seule son empreinte
SHA-256 est écrite — quiconque lit `session_ops` ne peut pas usurper de session,
et c'est précisément la table qu'un attaquant irait lire. Côté navigateur, il
vit en mémoire de page : ni `localStorage`, ni `sessionStorage`, ni cookie. Il
survivrait sinon à la fermeture de l'onglet et resterait lisible par tout script
injecté.

**Le tableau de bord ne porte que des agrégats.** Aucun identifiant, aucune
adresse, aucun UUID : un test le vérifie sur le corps brut de la réponse. Un
tableau de bord nominatif deviendrait un moyen commode de consulter des dossiers
sans passer par les routes qui enregistrent qui a regardé quoi. Chaque
consultation est elle-même journalisée, refus compris.

**La mesure porte le nom de ce qu'elle mesure.** FR-040 demande le NPS ; le
produit ne pose jamais la question « recommanderiez-vous ». Le tableau rend une
**note moyenne avec son nombre de notes**, et l'écart est écrit plutôt que
masqué derrière un intitulé emprunté.

## Suivi géolocalisé du trajet (Story 4.4, FR-019)

**La DPIA reste absente, et c'est le point bloquant.** Le tableau d'ouverture le
dit déjà : l'analyse d'impact (RGPD art. 35) doit précéder le traitement, pas le
suivre. Ce qui est décrit ci-dessous réduit le traitement à ce qui est
défendable ; cela ne remplace pas l'analyse, et le suivi ne doit pas être activé
sur des positions réelles avant qu'elle soit signée.

**Le consentement est spécifique à une intervention.** Une préférence de compte
vaudrait pour toutes les missions passées et futures : ce n'est pas un
consentement éclairé au sens de l'art. 4.11. Une ligne par Mission, donnée par le
prestataire et révocable à tout moment (art. 7 §3).

**Une révocation ne supprime pas la trace du consentement.** Effacer la ligne
ferait disparaître la preuve qu'un accord avait été donné, c'est-à-dire
exactement ce qu'un contrôle vient vérifier. La date du retrait est écrite à côté
de celle de l'accord.

**Le retrait vaut pour l'avenir.** Les positions déjà partagées restent jusqu'à
la purge : elles ont été transmises de plein gré et le demandeur s'est organisé
dessus. Les effacer rétroactivement lui retirerait ce sur quoi il compte, sans
rien rendre au prestataire qui les a partagées.

**La minimisation est appliquée à l'écriture, pas à l'affichage** (art. 5.1.c).
Les positions sont ramenées à une grille de cinquante mètres **avant** d'entrer
en base. Dégrader au moment de montrer laisserait la donnée fine là où une fuite
la prendrait et où une réquisition la trouverait. Un test interroge la table pour
le vérifier, parce que la réponse HTTP ne prouve rien sur ce qui est conservé.

**Vingt-quatre heures après la fin, il ne reste qu'une distance, une durée et un
nombre de relevés.** De quoi arbitrer un litige sur un déplacement, rien de quoi
reconstituer une journée. L'agrégation et la suppression sont une seule
instruction SQL : en deux, une panne entre les deux laisserait soit le chemin
sans la mesure, soit les deux.

**L'échéance se compte sur l'horloge du serveur.** L'heure déclarée par le
prestataire peut légitimement précéder l'enregistrement, mais l'adosser à un
délai de suppression la rendrait manipulable dans les deux sens.

**Le prestataire voit ce que le demandeur verra.** La réponse à un envoi de
position rend la position **dégradée**, et non celle qui a été transmise : la
maille cesse ainsi d'être une promesse invisible.

**Limite assumée : pas de carte.** Un point sur un plan se lit comme un pointé au
mètre, ce que la grille de cinquante mètres ne permet pas. La position est rendue
en clair avec sa marge annoncée.

## Cycle de vie d'une Mission : ce que l'historique enregistre (Story 4.3, FR-018)

**La position est facultative.** FR-018 demande la géolocalisation sur chaque
entrée d'historique. L'exiger rendrait l'autorisation de localisation de fait
obligatoire, alors que quelqu'un sans GPS doit pouvoir déclarer qu'il est
arrivé. Son absence est consignée comme telle, et le marqueur « hors zone » ne
vaut jamais vrai sans position : ne pas savoir où quelqu'un est n'est pas la
même chose que le savoir ailleurs.

**Sortir de la Région se consigne, ne refuse pas.** Un prestataire qui coupe par
le ring reste en intervention. L'alerte d'exploitation est journalisée **sans**
la position ni l'identifiant de Mission : le journal applicatif n'a pas à dire où
se trouve un prestataire.

**Deux horodatages.** Celui que le client déclare et celui où le serveur reçoit.
Une transition faite hors connexion garde ainsi sa date. Au-delà de cinq minutes
d'écart, la date est refusée : ce n'est plus un décalage de synchronisation mais
une date choisie.

**L'historique est append-only**, par déclencheur, comme la trace de matching.
Une preuve qu'on peut réécrire n'en est pas une. La bascule de statut et l'entrée
d'historique sont écrites dans la même transaction.

**L'avis d'avancement ne nomme pas le prestataire.** « Untel est en route » lu
par-dessus une épaule dit qui vient chez qui.

## Trace de matching : immuable, scellée, auditée (Story 3.8, AI Act art. 12)

**Immuabilité par déclencheur, pas par convention.** `UPDATE` et `DELETE` sur
`trace_matching` lèvent une exception, y compris ceux venant d'une cascade :
supprimer une Demande tracée échoue bruyamment plutôt que d'emporter sa trace.

**Signature chaînée.** Chaque ligne porte un HMAC-SHA256 de son contenu **et** de
la signature précédente. Un HMAC par ligne détecterait une modification mais pas
une suppression, et supprimer est exactement ce que ferait quelqu'un voulant
effacer un matching discriminatoire.

**Portée réelle.** La signature détecte une altération faite depuis la base. Elle
ne couvre **pas** une compromission du serveur, où la clé est lisible et permet
de resigner. Le stockage WORM tiers que FR-012 décrit lèverait cette limite ; il
demande un compte d'hébergement, hors du périmètre vitrine.

**Limite opérationnelle.** La chaîne est globale : une rotation de clé casse la
vérification à partir du premier maillon signé avec la nouvelle. Une rotation
demandera de conserver l'ancienne clé pour vérifier le segment antérieur.

**Clé optionnelle, et pourquoi.** Sans clé, la trace est écrite non signée : elle
explique toujours une décision, ce que l'AI Act exige, alors que l'absence de
trace ne s'explique pas. Les lignes non signées sont comptées à part dans le
rapport — les ranger avec les vérifiées produirait un rapport rassurant sans
preuve.

**Tension avec le droit à l'effacement (art. 17).** L'effacement d'un compte est
une anonymisation, donc aucune cascade ne touche la trace aujourd'hui. Une
suppression future échouerait sur le déclencheur, et ce serait la bonne réponse :
l'art. 17 §3 b) réserve le cas des traitements imposés par une obligation légale.
La trace ne porte ni nom, ni adresse, ni description — deux identifiants, un
score et une distance.

## Audit anti-biais : ce qui est mesuré, et ce qui ne peut pas l'être (Story 3.8)

FR-012 demande un rapport sur trois axes. Deux ne sont pas auditables, et ce
n'est pas une lacune :

- **Le genre n'est pas collecté.** L'auditer supposerait de le demander, donc de
  créer la donnée qui rendrait la discrimination possible.
- **L'« ethnie estimée » suppose de l'estimer**, typiquement depuis un nom :
  précisément la pratique que l'AI Act et le RGPD art. 9 proscrivent.

La garantie sur ces deux axes est structurelle, et plus forte qu'un audit
statistique : la fonction de score reçoit quatre nombres et ne peut discriminer
sur un attribut qu'on ne lui donne pas. Le rapport le dit explicitement.

**Le quartier est audité**, par maille d'environ un kilomètre : nombre de
Demandes, taux d'attribution, part sans réponse, et l'écart entre la maille la
mieux servie et la moins bien servie. La cause d'un écart est la densité de
prestataires, pas le score.

**k-anonymat, seuil à cinq.** Les mailles sous le seuil sont supprimées du
rapport et leur nombre est annoncé : les taire ferait passer une couverture
partielle pour complète. Elles restent comptées dans le total.

## Disponibilité : trois raisons de ne rien recevoir (Story 3.7)

Un prestataire peut être écarté du matching pour trois raisons distinctes : son
**statut** (en attente de contrôle, suspendu), sa **disponibilité** (« je suis en
congé »), et son **occupation** (une Mission en cours). Les confondre ferait
d'une pause une sanction. Seule la disponibilité se règle ; les deux autres sont
exposées, pour qu'un prestataire jamais sollicité puisse comprendre pourquoi
plutôt que d'en conclure que le service est cassé.

**Un prestataire déjà en Mission n'est plus proposé.** C'était un trou depuis la
Story 3.4 : il recevait des notifications qu'il ne pouvait qu'échouer à
accepter, et volait sa place à quelqu'un de libre. Le filtre est posé par la
base, pas appliqué après coup.

**Le rayon d'intervention appartient au prestataire.** Le tour de diffusion dit
jusqu'où la Demande cherche ; ce rayon dit jusqu'où le prestataire accepte
d'aller. Le défaut est le maximum : les fiches existantes n'ont jamais exprimé de
limite, et leur en prêter une les retirerait du service sans qu'elles aient rien
demandé.

**Non livré : les zones d'intervention disjointes.** Travailler à deux endroits
sans couvrir l'espace entre les deux demanderait un modèle géographique autre
qu'un point et un rayon. Ce qui est livré est la part actionnable : chacun règle
sa distance.

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
