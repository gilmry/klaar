//! Preuves photographiques d'une intervention (FR-020, Story 4.5).
//!
//! **Le stockage attend un seau ; la validation, non.** Chiffrer et déposer un
//! fichier demande OVH S3 et son KMS, absents. Décider si un fichier est
//! recevable — est-ce bien une image, pèse-t-elle moins que la borne, porte-t-elle
//! l'horodatage qui en fait une preuve, en a-t-on déjà assez — ne demande rien
//! d'autre que ce module. Et c'est cette partie-là qu'un dépôt d'objet ne fera
//! jamais à notre place.
//!
//! **Le type est décidé sur le contenu, jamais sur le nom ni sur
//! `Content-Type`.** Les deux sont donnés par celui qui téléverse. Un fichier
//! HTML nommé `photo.jpg` et annoncé `image/jpeg` finirait servi par le domaine
//! du service, où un navigateur l'exécuterait ; c'est la faille classique du
//! téléversement d'images, et elle ne se ferme qu'en lisant les premiers octets.
//!
//! **Tension assumée avec FR-019, et écrite plutôt que tue.** Le suivi de
//! position dégrade la géolocalisation à cinquante mètres pour ne pas dire où
//! quelqu'un habite. FR-020 demande l'inverse : une photo *avec* sa
//! géolocalisation EXIF, visible du demandeur, du prestataire et de
//! l'exploitation. Une photo de la chaudière d'un foyer, prise sur place,
//! porterait donc l'adresse au mètre — exactement ce que FR-019 protège. Ce
//! module exige l'**horodatage** comme preuve et rend la géolocalisation
//! **facultative** ; trancher pour de bon demande une décision produit, pas un
//! choix d'implémentation.

use chrono::{DateTime, Utc};
use std::fmt;

/// Taille maximale d'une preuve, en octets (FR-020 `@negative`).
pub const TAILLE_MAX_OCTETS: usize = 10 * 1024 * 1024;

/// Preuves par phase (FR-020 `@edge`).
///
/// Cinq. Au-delà, ce n'est plus une preuve mais un reportage, et le litige n'en
/// est pas mieux tranché.
pub const PREUVES_MAX_PAR_PHASE: usize = 5;

/// La phase à laquelle une preuve se rapporte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhasePreuve {
    Avant,
    Apres,
}

impl PhasePreuve {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Avant => "BEFORE",
            Self::Apres => "AFTER",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "BEFORE" => Some(Self::Avant),
            "AFTER" => Some(Self::Apres),
            _ => None,
        }
    }
}

/// Les formats acceptés.
///
/// **Trois, et pas un de plus.** Chaque format supplémentaire est un décodeur
/// de plus exposé à des fichiers hostiles. SVG en particulier est exclu : c'est
/// un document XML qui peut porter du script, et « image » y est un abus de
/// langage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatImage {
    Jpeg,
    Png,
    Webp,
}

impl FormatImage {
    pub fn type_mime(&self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
        }
    }

    /// Reconnaît le format aux premiers octets du fichier.
    ///
    /// **Sur le contenu, jamais sur le nom.** Le nom et le `Content-Type`
    /// viennent de celui qui téléverse ; les octets, non.
    pub fn reconnaitre(contenu: &[u8]) -> Option<Self> {
        if contenu.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some(Self::Jpeg);
        }
        if contenu.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Some(Self::Png);
        }
        // WebP : « RIFF » puis quatre octets de taille, puis « WEBP ».
        if contenu.len() >= 12 && contenu.starts_with(b"RIFF") && &contenu[8..12] == b"WEBP" {
            return Some(Self::Webp);
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreuveError {
    /// Le contenu n'est pas une image d'un format accepté (FR-020 `@negative`).
    TypeInvalide,
    /// Au-delà de la borne de taille (FR-020 `@negative`).
    TropVolumineuse { octets: usize },
    /// Fichier vide.
    Vide,
    /// Pas d'horodatage : sans lui, la photo ne prouve pas *quand*.
    HorodatageAbsent,
    /// Horodatage postérieur à la réception : une preuve ne vient pas du futur.
    HorodatageDansLeFutur,
    /// Le quota de la phase est atteint (FR-020 `@edge`).
    QuotaAtteint,
    /// La phase ne correspond pas à l'état de l'intervention.
    PhaseHorsPropos,
}

impl PreuveError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::TypeInvalide => "INVALID_FILE_TYPE",
            Self::TropVolumineuse { .. } => "FILE_TOO_LARGE",
            Self::Vide => "FILE_EMPTY",
            Self::HorodatageAbsent => "EXIF_REQUIRED",
            Self::HorodatageDansLeFutur => "EXIF_TIMESTAMP_INVALID",
            Self::QuotaAtteint => "MAX_EVIDENCE_REACHED",
            Self::PhaseHorsPropos => "EVIDENCE_PHASE_INVALID",
        }
    }
}

impl fmt::Display for PreuveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeInvalide => write!(f, "ce fichier n'est pas une image JPEG, PNG ou WebP"),
            Self::TropVolumineuse { octets } => {
                write!(f, "photo de {octets} octets, maximum {TAILLE_MAX_OCTETS}")
            }
            Self::Vide => write!(f, "fichier vide"),
            Self::HorodatageAbsent => {
                write!(
                    f,
                    "la photo ne porte pas d'horodatage : elle ne prouve pas quand"
                )
            }
            Self::HorodatageDansLeFutur => write!(f, "horodatage postérieur à la réception"),
            Self::QuotaAtteint => write!(
                f,
                "{PREUVES_MAX_PAR_PHASE} photos suffisent pour cette phase"
            ),
            Self::PhaseHorsPropos => {
                write!(
                    f,
                    "cette phase ne correspond pas à l'état de l'intervention"
                )
            }
        }
    }
}

impl std::error::Error for PreuveError {}

/// Ce que l'EXIF apporte, une fois lu.
///
/// **L'horodatage est exigé, la position ne l'est pas.** Voir la tension avec
/// FR-019 dans l'en-tête du module : une photo prise chez quelqu'un porte son
/// adresse au mètre, ce que le suivi de position s'interdit précisément.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetadonneesExif {
    pub pris_le: Option<DateTime<Utc>>,
    /// Vrai si le fichier porte une position. **Le booléen, pas la position** :
    /// ce module n'a pas à la manipuler, seulement à dire qu'elle est là — ce
    /// qui permet à l'écran d'en avertir celui qui téléverse.
    pub porte_une_position: bool,
}

/// Une preuve recevable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preuve {
    pub phase: PhasePreuve,
    pub format: FormatImage,
    pub taille_octets: usize,
    /// Empreinte SHA-256 du contenu, en hexadécimal.
    ///
    /// **Calculée avant chiffrement et conservée.** C'est elle qui permet de
    /// dire, des mois plus tard devant un litige, que le fichier rendu est bien
    /// celui qui a été déposé.
    pub empreinte: String,
    pub pris_le: DateTime<Utc>,
    pub porte_une_position: bool,
    pub recue_le: DateTime<Utc>,
}

/// Valide une preuve avant tout stockage.
///
/// `deja_pour_la_phase` est le nombre de preuves déjà acceptées pour cette
/// phase ; `phase_ouverte` dit si l'état de l'intervention l'autorise.
///
/// **L'ordre des contrôles suit le coût.** Type et taille d'abord — ils se
/// décident sur quelques octets et écartent l'essentiel de ce qui n'a rien à
/// faire là. L'empreinte, qui parcourt tout le fichier, vient en dernier.
pub fn valider(
    contenu: &[u8],
    phase: PhasePreuve,
    phase_ouverte: bool,
    deja_pour_la_phase: usize,
    exif: MetadonneesExif,
    maintenant: DateTime<Utc>,
) -> Result<Preuve, PreuveError> {
    if !phase_ouverte {
        return Err(PreuveError::PhaseHorsPropos);
    }
    if deja_pour_la_phase >= PREUVES_MAX_PAR_PHASE {
        return Err(PreuveError::QuotaAtteint);
    }
    if contenu.is_empty() {
        return Err(PreuveError::Vide);
    }
    if contenu.len() > TAILLE_MAX_OCTETS {
        return Err(PreuveError::TropVolumineuse {
            octets: contenu.len(),
        });
    }
    let format = FormatImage::reconnaitre(contenu).ok_or(PreuveError::TypeInvalide)?;

    let pris_le = exif.pris_le.ok_or(PreuveError::HorodatageAbsent)?;
    // Une preuve ne vient pas du futur. La tolérance est nulle : l'appareil du
    // prestataire et le serveur peuvent diverger de quelques secondes, mais une
    // photo datée de demain est une photo dont la date a été choisie.
    if pris_le > maintenant {
        return Err(PreuveError::HorodatageDansLeFutur);
    }

    Ok(Preuve {
        phase,
        format,
        taille_octets: contenu.len(),
        empreinte: empreinte_sha256(contenu),
        pris_le,
        porte_une_position: exif.porte_une_position,
        recue_le: maintenant,
    })
}

/// Empreinte SHA-256 en hexadécimal.
fn empreinte_sha256(contenu: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hacheur = Sha256::new();
    hacheur.update(contenu);
    hacheur
        .finalize()
        .iter()
        .map(|o| format!("{o:02x}"))
        .collect()
}

/// Vrai si la paire avant/après est complète (FR-020 `@happy`).
///
/// **Les deux, ou rien.** Une seule photo « après » ne prouve pas qu'un travail
/// a été fait : elle montre un état, pas un changement. C'est la paire qui a
/// une valeur devant un litige.
pub fn paire_complete(avant: usize, apres: usize) -> bool {
    avant > 0 && apres > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, 9, 0, 0).unwrap()
    }

    /// Un JPEG minimal : les octets qui comptent sont les trois premiers.
    fn jpeg() -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0];
        v.extend_from_slice(&[0u8; 64]);
        v
    }

    fn png() -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&[0u8; 64]);
        v
    }

    fn webp() -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&[0u8; 4]);
        v.extend_from_slice(b"WEBP");
        v.extend_from_slice(&[0u8; 64]);
        v
    }

    fn exif() -> MetadonneesExif {
        MetadonneesExif {
            pris_le: Some(t0() - Duration::minutes(5)),
            porte_une_position: false,
        }
    }

    #[test]
    fn happy_une_photo_recevable_est_acceptee_et_empreintee() {
        let p = valider(&jpeg(), PhasePreuve::Avant, true, 0, exif(), t0()).unwrap();
        assert_eq!(p.format, FormatImage::Jpeg);
        assert_eq!(p.phase, PhasePreuve::Avant);
        // Une empreinte SHA-256 fait 64 caractères hexadécimaux.
        assert_eq!(p.empreinte.len(), 64);
        assert!(p.empreinte.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn happy_les_trois_formats_sont_reconnus() {
        for (contenu, attendu) in [
            (jpeg(), FormatImage::Jpeg),
            (png(), FormatImage::Png),
            (webp(), FormatImage::Webp),
        ] {
            assert_eq!(FormatImage::reconnaitre(&contenu), Some(attendu));
        }
    }

    #[test]
    fn security_le_type_est_decide_sur_le_contenu_et_non_sur_le_nom() {
        // Le cœur du contrôle : un fichier HTML nommé `photo.jpg` et annoncé
        // `image/jpeg` finirait servi par le domaine du service, où un
        // navigateur l'exécuterait. Ni le nom ni le `Content-Type` n'entrent
        // dans cette fonction — il n'y a rien à tromper.
        for hostile in [
            &b"<html><script>alert(1)</script></html>"[..],
            b"%PDF-1.7",
            b"GIF89a",
            // SVG : un document XML qui peut porter du script. « Image » y est
            // un abus de langage.
            br#"<svg xmlns="http://www.w3.org/2000/svg"><script>1</script></svg>"#,
            // Presque un JPEG : deux octets sur trois.
            &[0xFF, 0xD8, 0x00, 0x00],
        ] {
            assert_eq!(FormatImage::reconnaitre(hostile), None);
            assert_eq!(
                valider(hostile, PhasePreuve::Avant, true, 0, exif(), t0()),
                Err(PreuveError::TypeInvalide),
                "contenu accepté à tort : {:?}",
                &hostile[..hostile.len().min(12)]
            );
        }
    }

    #[test]
    fn security_un_webp_tronque_n_est_pas_reconnu() {
        // « RIFF » seul ne suffit pas : c'est un conteneur générique, qui porte
        // aussi bien du son. Sans les quatre octets « WEBP », ce n'est pas une
        // image.
        let mut faux = b"RIFF".to_vec();
        faux.extend_from_slice(&[0u8; 4]);
        faux.extend_from_slice(b"WAVE");
        assert_eq!(FormatImage::reconnaitre(&faux), None);
        // Et un préfixe trop court ne doit pas faire déborder l'index.
        assert_eq!(FormatImage::reconnaitre(b"RIFF"), None);
        assert_eq!(FormatImage::reconnaitre(b""), None);
    }

    #[test]
    fn negative_une_photo_trop_volumineuse_est_refusee() {
        let enorme = vec![0xFFu8; TAILLE_MAX_OCTETS + 1];
        assert!(matches!(
            valider(&enorme, PhasePreuve::Avant, true, 0, exif(), t0()),
            Err(PreuveError::TropVolumineuse { .. })
        ));
    }

    #[test]
    fn edge_la_borne_exacte_de_taille_passe() {
        let mut limite = jpeg();
        limite.resize(TAILLE_MAX_OCTETS, 0);
        assert!(valider(&limite, PhasePreuve::Avant, true, 0, exif(), t0()).is_ok());
    }

    #[test]
    fn negative_sans_horodatage_la_photo_ne_prouve_rien() {
        // FR-020 `@negative` : 422 `EXIF_REQUIRED`. Une photo sans date ne dit
        // pas *quand* l'état montré était celui-là.
        let sans = MetadonneesExif {
            pris_le: None,
            porte_une_position: true,
        };
        assert_eq!(
            valider(&jpeg(), PhasePreuve::Avant, true, 0, sans, t0()),
            Err(PreuveError::HorodatageAbsent)
        );
    }

    #[test]
    fn security_une_photo_datee_du_futur_est_refusee() {
        // Une date choisie plutôt que constatée : c'est le cas d'une preuve
        // fabriquée après coup pour couvrir un délai.
        let futur = MetadonneesExif {
            pris_le: Some(t0() + Duration::seconds(1)),
            porte_une_position: false,
        };
        assert_eq!(
            valider(&jpeg(), PhasePreuve::Avant, true, 0, futur, t0()),
            Err(PreuveError::HorodatageDansLeFutur)
        );
    }

    #[test]
    fn edge_le_quota_de_cinq_par_phase_est_tenu() {
        // FR-020 `@edge` : la sixième est refusée.
        assert!(valider(
            &jpeg(),
            PhasePreuve::Avant,
            true,
            PREUVES_MAX_PAR_PHASE - 1,
            exif(),
            t0()
        )
        .is_ok());
        assert_eq!(
            valider(
                &jpeg(),
                PhasePreuve::Avant,
                true,
                PREUVES_MAX_PAR_PHASE,
                exif(),
                t0()
            ),
            Err(PreuveError::QuotaAtteint)
        );
    }

    #[test]
    fn edge_le_quota_est_par_phase_et_non_global() {
        // Cinq photos « avant » ne doivent pas empêcher la première « après » :
        // ce sont deux preuves de choses différentes.
        assert!(valider(&jpeg(), PhasePreuve::Apres, true, 0, exif(), t0()).is_ok());
    }

    #[test]
    fn negative_une_phase_fermee_refuse_la_preuve() {
        assert_eq!(
            valider(&jpeg(), PhasePreuve::Apres, false, 0, exif(), t0()),
            Err(PreuveError::PhaseHorsPropos)
        );
    }

    #[test]
    fn negative_un_fichier_vide_est_refuse_avant_tout_le_reste() {
        assert_eq!(
            valider(&[], PhasePreuve::Avant, true, 0, exif(), t0()),
            Err(PreuveError::Vide)
        );
    }

    #[test]
    fn security_l_empreinte_change_au_moindre_octet() {
        // C'est elle qui dira, des mois plus tard devant un litige, que le
        // fichier rendu est bien celui qui a été déposé.
        let a = valider(&jpeg(), PhasePreuve::Avant, true, 0, exif(), t0()).unwrap();
        let mut modifie = jpeg();
        *modifie.last_mut().unwrap() ^= 1;
        let b = valider(&modifie, PhasePreuve::Avant, true, 0, exif(), t0()).unwrap();
        assert_ne!(a.empreinte, b.empreinte);

        // Et elle est stable : le même contenu donne la même empreinte.
        let c = valider(&jpeg(), PhasePreuve::Avant, true, 0, exif(), t0()).unwrap();
        assert_eq!(a.empreinte, c.empreinte);
    }

    #[test]
    fn security_l_empreinte_est_celle_du_contenu_connu() {
        // Vecteur de contrôle : SHA-256 de la chaîne vide, valeur publiée.
        assert_eq!(
            empreinte_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn edge_la_position_exif_est_signalee_sans_etre_manipulee() {
        // Tension avec FR-019, documentée dans l'en-tête : une photo prise chez
        // quelqu'un porte son adresse au mètre, ce que le suivi de position
        // s'interdit. Le module dit qu'elle est là ; il ne la lit pas.
        let avec = MetadonneesExif {
            pris_le: Some(t0()),
            porte_une_position: true,
        };
        let p = valider(&jpeg(), PhasePreuve::Avant, true, 0, avec, t0()).unwrap();
        assert!(p.porte_une_position);
    }

    #[test]
    fn edge_une_paire_incomplete_ne_prouve_pas_un_changement() {
        // Une seule photo « après » montre un état, pas un changement.
        assert!(!paire_complete(0, 3));
        assert!(!paire_complete(3, 0));
        assert!(!paire_complete(0, 0));
        assert!(paire_complete(1, 1));
    }

    #[test]
    fn edge_le_vocabulaire_fait_l_aller_retour() {
        for phase in [PhasePreuve::Avant, PhasePreuve::Apres] {
            assert_eq!(PhasePreuve::parse(phase.as_str()), Some(phase));
        }
        assert_eq!(PhasePreuve::parse("PENDANT"), None);
    }
}
