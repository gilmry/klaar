// fake-indexeddb fournit une implémentation conforme d'IndexedDB en mémoire.
// Sans elle, les tests de la queue hors-ligne ne testeraient qu'un bouchon,
// c'est-à-dire rien.
import "fake-indexeddb/auto";
