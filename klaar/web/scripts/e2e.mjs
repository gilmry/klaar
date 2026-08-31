#!/usr/bin/env node
/**
 * Lance les tests e2e avec un affichage, quand il en faut un.
 *
 * **Pourquoi ce détour plutôt qu'un `playwright test` direct.** Chromium sans
 * affichage n'a pas de service de notification : `Notification.permission` rend
 * « denied » même après que Playwright a accordé la permission — la requête
 * `navigator.permissions.query` rend pourtant « granted », ce qui rend le
 * diagnostic peu évident. Les six cas de `push.spec.ts` échouaient donc sur une
 * capacité absente du navigateur de test, pas sur un défaut du code.
 *
 * Ce script fournit un serveur X virtuel quand il n'y en a pas. Sans `xvfb-run`
 * installé, il ne fait pas semblant : il pose `KLAAR_SANS_AFFICHAGE=1`, et les
 * cas concernés se sautent **en le disant**. Un test sauté qui explique
 * pourquoi vaut mieux qu'un test rouge qui ferait chercher une régression, et
 * mieux qu'un test vert qui n'aurait rien vérifié.
 */
import { spawn, spawnSync } from "node:child_process";

const args = process.argv.slice(2);

function disponible(commande) {
  return spawnSync("command", ["-v", commande], { shell: true, stdio: "ignore" }).status === 0;
}

function lancer(commande, arguments_, environnement = {}) {
  const enfant = spawn(commande, arguments_, {
    stdio: "inherit",
    env: { ...process.env, ...environnement },
  });
  enfant.on("exit", (code, signal) => process.exit(signal ? 1 : (code ?? 1)));
}

if (process.env.DISPLAY) {
  lancer("npx", ["playwright", "test", ...args]);
} else if (disponible("xvfb-run")) {
  // `-a` : choisir un numéro d'écran libre. Sans lui, deux exécutions
  // concurrentes se disputent :99 et la seconde échoue à l'ouverture.
  lancer("xvfb-run", ["-a", "npx", "playwright", "test", ...args]);
} else {
  console.warn(
    "\n⚠ Ni DISPLAY ni xvfb-run : les tests de notifications seront sautés.\n" +
      "  Chromium sans affichage ne délivre aucune notification.\n" +
      "  Sur Debian/Ubuntu : sudo apt-get install xvfb\n",
  );
  lancer("npx", ["playwright", "test", ...args], { KLAAR_SANS_AFFICHAGE: "1" });
}
