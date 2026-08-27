-- Story 2.1 — catalogue MVP : 5 secteurs et leurs Skills (FR-008).
--
-- Les trois traductions sont **obligatoires** en base, pas seulement dans le
-- domaine. Bruxelles est officiellement bilingue : une entrée sans néerlandais
-- n'est pas une entrée incomplète, c'est une entrée qui ne devrait pas exister.
-- Une contrainte `NOT NULL` seule laisserait passer la chaîne vide, d'où le
-- `CHECK` sur la longueur après suppression des espaces.

CREATE TABLE secteur (
    -- Le code voyage dans les URL, les statistiques et les exports : il ne se
    -- renomme pas. Le format restreint (minuscules, chiffres, tirets) survit à
    -- une URL, à un nom de colonne et à un CSV sans échappement.
    code       TEXT PRIMARY KEY
        CONSTRAINT secteur_code_slug CHECK (code ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
    libelle_fr TEXT NOT NULL CONSTRAINT secteur_fr_non_vide CHECK (btrim(libelle_fr) <> ''),
    libelle_nl TEXT NOT NULL CONSTRAINT secteur_nl_non_vide CHECK (btrim(libelle_nl) <> ''),
    libelle_en TEXT NOT NULL CONSTRAINT secteur_en_non_vide CHECK (btrim(libelle_en) <> ''),
    -- Ordre d'affichage. Explicite plutôt qu'alphabétique : l'ordre
    -- alphabétique change d'une langue à l'autre, et le catalogue afficherait
    -- les mêmes secteurs dans un ordre différent selon la langue choisie.
    ordre      INT  NOT NULL
);

CREATE TABLE skill (
    code         TEXT PRIMARY KEY
        CONSTRAINT skill_code_slug CHECK (code ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
    secteur_code TEXT NOT NULL REFERENCES secteur (code) ON DELETE CASCADE,
    libelle_fr   TEXT NOT NULL CONSTRAINT skill_fr_non_vide CHECK (btrim(libelle_fr) <> ''),
    libelle_nl   TEXT NOT NULL CONSTRAINT skill_nl_non_vide CHECK (btrim(libelle_nl) <> ''),
    libelle_en   TEXT NOT NULL CONSTRAINT skill_en_non_vide CHECK (btrim(libelle_en) <> ''),
    ordre        INT  NOT NULL
);

CREATE INDEX skill_secteur_idx ON skill (secteur_code, ordre);

-- --- Amorçage MVP -----------------------------------------------------------
--
-- Les cinq secteurs nommés par le PRD. Les Skills, eux, ne figurent nulle part
-- dans les livrables de conception : cette liste est une **proposition** tirée
-- des interventions de dépannage courantes à Bruxelles, à valider avec le
-- métier avant toute mise en service. Elle est ici pour que le catalogue existe
-- et se teste, pas parce qu'elle fait autorité.

INSERT INTO secteur (code, libelle_fr, libelle_nl, libelle_en, ordre) VALUES
    ('plomberie',   'Plomberie',   'Loodgieterij',   'Plumbing',      1),
    ('serrurerie',  'Serrurerie',  'Slotenmakerij',  'Locksmithing',  2),
    -- « électricité » se translittère en « electricite » : le libellé porte
    -- l'accent, le code non.
    ('electricite', 'Électricité', 'Elektriciteit',  'Electricity',   3),
    ('auto',        'Auto',        'Auto',           'Car',           4),
    ('livraison',   'Livraison',   'Levering',       'Delivery',      5);

INSERT INTO skill (code, secteur_code, libelle_fr, libelle_nl, libelle_en, ordre) VALUES
    ('fuite-eau',             'plomberie',   'Fuite d''eau',              'Waterlek',                'Water leak',           1),
    ('debouchage',            'plomberie',   'Débouchage',                'Ontstopping',             'Unblocking',           2),
    ('chauffe-eau',           'plomberie',   'Chauffe-eau',               'Boiler',                  'Water heater',         3),
    ('sanitaire',             'plomberie',   'Sanitaire',                 'Sanitair',                'Sanitary fittings',    4),

    ('ouverture-porte',       'serrurerie',  'Ouverture de porte',        'Deuropening',             'Door opening',         1),
    ('remplacement-cylindre', 'serrurerie',  'Remplacement de cylindre',  'Cilindervervanging',      'Cylinder replacement', 2),
    ('blindage-porte',        'serrurerie',  'Blindage de porte',         'Deurbeveiliging',         'Door reinforcement',   3),

    ('panne-courant',         'electricite', 'Panne de courant',          'Stroomstoring',           'Power outage',         1),
    ('tableau-electrique',    'electricite', 'Tableau électrique',        'Elektrische kast',        'Electrical panel',     2),
    ('eclairage',             'electricite', 'Éclairage',                 'Verlichting',             'Lighting',             3),
    ('prise-interrupteur',    'electricite', 'Prise et interrupteur',     'Stopcontact en schakelaar', 'Socket and switch',  4),

    ('depannage-batterie',    'auto',        'Dépannage batterie',        'Batterijhulp',            'Battery assistance',   1),
    ('crevaison',             'auto',        'Crevaison',                 'Lekke band',              'Flat tyre',            2),
    ('remorquage',            'auto',        'Remorquage',                'Sleepdienst',             'Towing',               3),
    ('ouverture-vehicule',    'auto',        'Ouverture de véhicule',     'Voertuigopening',         'Vehicle opening',      4),

    ('course-urgente',        'livraison',   'Course urgente',            'Spoedkoerier',            'Urgent courier',       1),
    ('demenagement-leger',    'livraison',   'Déménagement léger',        'Kleine verhuis',          'Light removal',        2),
    ('encombrant',            'livraison',   'Enlèvement d''encombrant',  'Ophalen van grofvuil',    'Bulky waste removal',  3);
