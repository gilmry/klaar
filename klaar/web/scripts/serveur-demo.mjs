/**
 * Serveur des parcours filmés : fichiers statiques + proxy vers l'API.
 *
 * **Pourquoi il existe.** Le front appelle `/api/v1` en relatif. Les tests de
 * vérification simulent ces appels et n'ont besoin de rien ; les parcours
 * filmés, eux, doivent atteindre le vrai service. Trois façons de le faire :
 *
 *   - pointer le front sur `http://localhost:8080` — il faudrait alors du CORS
 *     sur l'API, c'est-à-dire relâcher une garantie de production pour une
 *     démonstration ;
 *   - intercepter les requêtes dans le navigateur — le parcours ne montrerait
 *     plus le vrai chemin réseau ;
 *   - servir les deux sur la même origine, ce que fait ce script.
 *
 * La troisième est la seule qui reproduise le déploiement réel, où un proxy
 * inverse met le site et l'API derrière un même nom. C'est donc celle-ci.
 *
 * Sans dépendance : `node:http` suffit, et ajouter un paquet pour quarante
 * lignes irait contre la sobriété que ce projet revendique.
 */
import { createServer } from "node:http";
import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import { join, extname, normalize } from "node:path";

const PORT = Number(process.env.KLAAR_DEMO_PORT ?? 4321);
const RACINE = new URL("../dist/", import.meta.url).pathname;
const API = process.env.KLAAR_API_URL ?? "http://127.0.0.1:8080";

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".webmanifest": "application/manifest+json; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".ico": "image/x-icon",
  ".woff2": "font/woff2",
};

/** Relaie une requête vers l'API, en-têtes et corps compris. */
async function relayer(req, res) {
  const corps =
    req.method === "GET" || req.method === "HEAD"
      ? undefined
      : await new Promise((resoudre) => {
          const morceaux = [];
          req.on("data", (m) => morceaux.push(m));
          req.on("end", () => resoudre(Buffer.concat(morceaux)));
        });

  const entetes = { ...req.headers };
  // L'hôte doit être celui de l'API, pas celui du proxy.
  delete entetes.host;

  try {
    const reponse = await fetch(new URL(req.url, API), {
      method: req.method,
      headers: entetes,
      body: corps,
      redirect: "manual",
    });
    const sortants = {};
    reponse.headers.forEach((valeur, nom) => {
      // `content-encoding` mentirait : `fetch` a déjà décompressé.
      if (nom !== "content-encoding" && nom !== "content-length") sortants[nom] = valeur;
    });
    res.writeHead(reponse.status, sortants);
    res.end(Buffer.from(await reponse.arrayBuffer()));
  } catch (e) {
    // 502 et non 500 : c'est l'amont qui manque, pas ce serveur.
    res.writeHead(502, { "content-type": "application/json" });
    res.end(JSON.stringify({ code: "UPSTREAM_UNAVAILABLE", detail: String(e) }));
  }
}

async function servir(req, res) {
  if (req.url.startsWith("/api/")) return relayer(req, res);

  const chemin = new URL(req.url, "http://x").pathname;
  // `normalize` puis vérification du préfixe : sans cela, `/../../etc/passwd`
  // sortirait de la racine servie.
  const candidats = [join(RACINE, normalize(chemin)), join(RACINE, normalize(chemin), "index.html")];
  for (const candidat of candidats) {
    if (!candidat.startsWith(RACINE)) continue;
    try {
      const info = await stat(candidat);
      if (!info.isFile()) continue;
      res.writeHead(200, {
        "content-type": TYPES[extname(candidat)] ?? "application/octet-stream",
        // Aucune mise en cache : une démonstration doit montrer le build
        // courant, pas celui d'hier.
        "cache-control": "no-store",
        // Le service worker ne s'enregistre que sur une origine sécurisée ;
        // `localhost` en est une par convention, rien à ajouter.
      });
      createReadStream(candidat).pipe(res);
      return;
    } catch {
      // Candidat suivant.
    }
  }
  res.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
  res.end("introuvable");
}

createServer(servir).listen(PORT, "127.0.0.1", () => {
  console.log(`parcours filmés : http://127.0.0.1:${PORT} — API relayée vers ${API}`);
});
