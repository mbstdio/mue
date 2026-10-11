use gpui::{App, Global, Window};
use gpui_component::{ActiveTheme, Theme, ThemeMode};
use mue_core::profiles::{GeneralSettings, InterfaceLanguage, ThemePreference};

#[derive(Clone, Copy)]
pub struct Appearance(pub ThemePreference);
impl Global for Appearance {}

pub fn apply(settings: &GeneralSettings, cx: &mut App) {
    cx.set_global(Appearance(settings.theme));
    let mode = match settings.theme {
        ThemePreference::System => cx.window_appearance().into(),
        ThemePreference::Light => ThemeMode::Light,
        ThemePreference::Dark => ThemeMode::Dark,
    };
    if cx.theme().mode != mode {
        Theme::change(mode, None, cx);
    }
    gpui_component::set_locale(if is_french(settings.language) {
        "fr"
    } else {
        "en"
    });
}

pub fn configure_window(window: &mut Window, cx: &mut App) {
    if cx.global::<Appearance>().0 == ThemePreference::System {
        Theme::change(window.appearance(), Some(window), cx);
    }
    crate::platform::update_titlebar(window, cx);
    window
        .observe_window_appearance(|window, cx| {
            if cx.global::<Appearance>().0 == ThemePreference::System {
                Theme::change(window.appearance(), Some(window), cx);
            }
            crate::platform::update_titlebar(window, cx);
        })
        .detach();
    window
        .observe_global::<Theme>(cx, |window, cx| {
            crate::platform::update_titlebar(window, cx);
            window.refresh();
        })
        .detach();
}

fn is_french(language: InterfaceLanguage) -> bool {
    match language {
        InterfaceLanguage::English => false,
        InterfaceLanguage::French => true,
        InterfaceLanguage::System => crate::platform::system_language_is_french(),
    }
}

pub fn text(language: InterfaceLanguage, english: &'static str) -> &'static str {
    if !is_french(language) {
        return english;
    }
    match english {
        "General" => "Général",
        "Image" => "Image",
        "Video" => "Vidéo",
        "Audio" => "Audio",
        "Configure audio formats and saved conversion profiles." => {
            "Configurez les formats audio et les profils enregistrés."
        }
        "Audio bitrate (kbps)" => "Débit audio (kbit/s)",
        "MP3 bitrates: 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320 kbps." => {
            "Débits MP3 : 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320 kbit/s."
        }
        "Opus bitrate: 16–510 kbps." => "Débit Opus : 16–510 kbit/s.",
        "Audio bitrate: 16–512 kbps." => "Débit audio : 16–512 kbit/s.",
        "WAV uses uncompressed 16-bit PCM. Higher source precision is reduced to 16 bits." => {
            "WAV utilise du PCM 16 bits non compressé. Une précision source supérieure est réduite à 16 bits."
        }
        "FLAC uses lossless compression. Converting a lossy source does not restore lost quality." => {
            "FLAC utilise une compression sans perte. Convertir une source avec perte ne restaure pas la qualité perdue."
        }
        "The first audio stream is converted. Embedded artwork is not copied. Sample rate and channels follow encoder support." => {
            "La première piste audio est convertie. Les pochettes intégrées ne sont pas copiées. La fréquence et les canaux dépendent de l’encodeur."
        }
        "Example: {filename}-{profile}.{ext}. The output extension is added automatically if missing." => {
            "Exemple : {filename}-{profile}.{ext}. L’extension de sortie est ajoutée automatiquement si elle manque."
        }
        "SETTINGS" => "PARAMÈTRES",
        "Customize appearance, startup and conversion notifications." => {
            "Personnalisez l’apparence, le démarrage et le suivi des conversions."
        }
        "Configure image formats and saved conversion profiles." => {
            "Configurez les formats d’image et les profils enregistrés."
        }
        "Configure video formats and saved conversion profiles." => {
            "Configurez les formats vidéo et les profils enregistrés."
        }
        "Appearance" => "Apparence",
        "Theme" => "Thème",
        "System" => "Système",
        "Light" => "Clair",
        "Dark" => "Sombre",
        "Interface language" => "Langue de l’interface",
        "English" => "Anglais",
        "French" => "Français",
        "Applied and saved immediately." => "Appliqué et enregistré immédiatement.",
        "Startup" => "Démarrage",
        "Launch at sign-in" => "Lancer à l’ouverture de session",
        "Mue starts in the notification area. Closing settings leaves it running." => {
            "Mue démarre dans la zone de notification et reste actif à la fermeture des paramètres."
        }
        "Conversions" => "Conversions",
        "Convert files" => "Convertir des fichiers",
        "Conversion progress" => "Suivi des conversions",
        "Add files…" => "Ajouter des fichiers…",
        "Drop files here" => "Déposez vos fichiers ici",
        "Images, videos and audio · or use the + button" => {
            "Images, vidéos et audio · ou utilisez le bouton +"
        }
        "Choose a profile" => "Choisir un profil",
        "Choose a profile before converting" => "Choisissez un profil avant de convertir",
        "Profile no longer available" => "Profil indisponible : choisissez-en un autre",
        "Apply a profile…" => "Appliquer un profil…",
        "Apply a profile to selected files" => "Appliquer un profil aux fichiers sélectionnés",
        "Select files of the same type to apply a profile." => {
            "Sélectionnez des fichiers du même type pour appliquer un profil."
        }
        "Select all" => "Tout sélectionner",
        "Deselect all" => "Tout désélectionner",
        "Select file" => "Sélectionner le fichier",
        "Selected" => "sélectionné(s)",
        "Prepared" => "en préparation",
        "Ready" => "Prêt(s)",
        "Convert this file" => "Convertir ce fichier",
        "Convert ready files" => "Convertir les fichiers prêts",
        "Remove file" => "Retirer le fichier",
        "Return to list" => "Remettre dans la liste",
        "Remove selected files" => "Retirer les fichiers sélectionnés",
        "Unsupported or unavailable file" => "Fichier non pris en charge ou inaccessible",
        "The queue is full (1000 files maximum)" => "La file est pleine (1 000 fichiers maximum)",
        "Automatically hide completed conversions" => {
            "Masquer automatiquement les conversions terminées"
        }
        "Hide immediately when all conversions finish. Errors remain visible." => {
            "Masquer dès la fin des conversions. Les erreurs restent visibles."
        }
        "Show queue" => "Afficher la file",
        "Hide queue" => "Réduire la file",
        "Clear finished conversions" => "Effacer les conversions terminées",
        "About" => "À propos",
        "Originals are preserved. Outputs are saved next to the source without overwriting files." => {
            "Les originaux sont conservés. Les résultats sont enregistrés à côté de la source sans écraser de fichiers."
        }
        "Open settings folder" => "Ouvrir le dossier des paramètres",
        "Show conversions" => "Afficher les conversions",
        "Quit Mue" => "Quitter Mue",
        "Direct conversion defaults" => "Réglages de conversion directe",
        "Saved profiles" => "Profils enregistrés",
        "No profiles yet. Create one below." => "Aucun profil. Créez-en un ci-dessous.",
        "+ New profile" => "+ Nouveau profil",
        "Direct conversion settings" => "Réglages de conversion directe",
        "Conversion profile" => "Profil de conversion",
        "Originals are preserved. Maximum sizes never upscale or distort the source." => {
            "Les originaux sont conservés. Les dimensions maximales n’agrandissent ni ne déforment la source."
        }
        "Name" => "Nom",
        "Output format" => "Format de sortie",
        "Output filename template" => "Modèle du nom de sortie",
        "Click a variable to insert it at the cursor or replace the selected text." => {
            "Cliquez sur une variable pour l’insérer à la position du curseur ou remplacer le texte sélectionné."
        }
        "Original filename without extension" => "Nom du fichier original sans extension",
        "Output extension" => "Extension de sortie",
        "Profile name" => "Nom du profil",
        "Converted file width" => "Largeur du fichier converti",
        "Converted file height" => "Hauteur du fichier converti",
        "Original file width" => "Largeur du fichier original",
        "Original file height" => "Hauteur du fichier original",
        "Dimensions are unavailable for audio." => {
            "Les dimensions ne sont pas disponibles pour l’audio."
        }
        "Maximum width (blank = original)" => "Largeur maximale (vide = original)",
        "Maximum height (blank = original)" => "Hauteur maximale (vide = original)",
        "PNG compression (0–9, lossless)" => "Compression PNG (0–9, sans perte)",
        "Image quality (1–100)" => "Qualité d’image (1–100)",
        "Transparency background (RGB hex)" => "Fond de transparence (hex RVB)",
        "CRF (lower = higher quality)" => "CRF (plus bas = meilleure qualité)",
        "VP9 maximum bitrate is available in CBR or two-pass VBR." => {
            "Le débit maximum VP9 est disponible en CBR ou VBR deux passes."
        }
        "Rate control" => "Mode de débit",
        "Constant quality (CRF)" => "Qualité constante (CRF)",
        "VBR — 1 pass" => "VBR — 1 passe",
        "VBR — 2 passes" => "VBR — 2 passes",
        "Target video bitrate (kbps)" => "Débit vidéo cible (kbit/s)",
        "Maximum video bitrate (kbps, blank = unlimited)" => {
            "Débit vidéo maximum (kbit/s, vide = illimité)"
        }
        "CBR regulates bitrate around the target; maximum equals target." => {
            "Le CBR régule le débit autour de la cible ; le maximum est égal à la cible."
        }
        "VP9 maximum limits the average GOP bitrate, not individual peaks." => {
            "Le maximum VP9 limite le débit moyen des groupes d’images, pas les pics individuels."
        }
        "Two passes analyze the source first, then encode; conversion takes longer." => {
            "Les deux passes analysent d’abord la source, puis encodent ; la conversion prend plus de temps."
        }
        "Maximum FPS (blank = original)" => "FPS maximum (vide = original)",
        "Encoding speed" => "Vitesse d’encodage",
        "Fast" => "Rapide",
        "Balanced" => "Équilibrée",
        "Slow" => "Lente",
        "Audio bitrate (16–512 kbps)" => "Débit audio (16–512 kbit/s)",
        "Audio: keep" => "Audio : conserver",
        "Audio: remove" => "Audio : supprimer",
        "Metadata: keep when supported" => "Métadonnées : conserver si possible",
        "Metadata: remove" => "Métadonnées : supprimer",
        "Save changes" => "Enregistrer",
        "Duplicate as profile" => "Dupliquer en profil",
        "Delete profile" => "Supprimer le profil",
        "Convert files…" => "Convertir des fichiers…",
        "Convert with Mue" => "Convertir avec Mue",
        "Saved. Reopen the context menu to see changes." => {
            "Enregistré. Rouvrez le menu contextuel pour voir les changements."
        }
        "Preferences saved." => "Préférences enregistrées.",
        "Conversion added to queue." => "Conversion ajoutée à la file.",
        "No conversions yet." => "Aucune conversion.",
        "Waiting" => "En attente",
        "Converting…" => "Conversion…",
        "Completed" => "Terminée",
        "Failed" => "Échec",
        "Cancelled" => "Annulée",
        "Cancel" => "Annuler",
        "Show file" => "Afficher le fichier",
        "Hide" => "Masquer",
        "Settings" => "Paramètres",
        _ => english,
    }
}
