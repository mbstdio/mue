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
        "Maximum width (blank = original)" => "Largeur maximale (vide = original)",
        "Maximum height (blank = original)" => "Hauteur maximale (vide = original)",
        "PNG compression (0–9, lossless)" => "Compression PNG (0–9, sans perte)",
        "Image quality (1–100)" => "Qualité d’image (1–100)",
        "Transparency background (RGB hex)" => "Fond de transparence (hex RVB)",
        "CRF (lower = higher quality)" => "CRF (plus bas = meilleure qualité)",
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
