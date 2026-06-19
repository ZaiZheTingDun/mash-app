use super::{
    classify_enhancement_route, norm_rect_area, normalize_text, parse_selected_count,
    scale_level_3_decision, EnhancementRoute, EnhancementStatus, EnhancementTopScreen,
    EnhancementVariant, ProbeSnapshot, ScaleLevel3Decision, ASCENSION_ENTRY_OCR_REGION,
    DIALOG_CLASSIFIER_REGION, FILTER_DIALOG_REGION,
};

#[test]
fn parse_selected_count_reads_counter() {
    assert_eq!(parse_selected_count("選択済み: 20/20"), Some(20));
    assert_eq!(parse_selected_count("20/20"), Some(20));
    assert_eq!(parse_selected_count("選択済 7/20"), Some(7));
    assert_eq!(parse_selected_count("選択済み：\n020"), Some(0));
    assert_eq!(parse_selected_count("選択済み：\n720"), Some(7));
    assert_eq!(parse_selected_count("選択済み：\n2020"), Some(20));
}

#[test]
fn normalize_text_drops_spacing_and_punctuation() {
    assert_eq!(normalize_text(" Exp. UP "), "expup");
    assert_eq!(normalize_text("Lv. 80/90"), "lv80/90");
}

#[test]
fn hot_ocr_regions_stay_narrow() {
    assert!(norm_rect_area(DIALOG_CLASSIFIER_REGION) < norm_rect_area(FILTER_DIALOG_REGION));
    assert!(norm_rect_area(ASCENSION_ENTRY_OCR_REGION) < 0.05);
}

#[test]
fn template_route_classifies_main_menu_states() {
    assert_eq!(
        classify_enhancement_route(&ProbeSnapshot::from_keys(&["button_notification"])),
        Some(EnhancementRoute {
            screen: EnhancementTopScreen::Main,
            variant: EnhancementVariant::Main,
            status: EnhancementStatus::None,
        })
    );
    assert_eq!(
        classify_enhancement_route(&ProbeSnapshot::from_keys(&[
            "button_notification",
            "button_enhancement"
        ])),
        Some(EnhancementRoute {
            screen: EnhancementTopScreen::Main,
            variant: EnhancementVariant::Main,
            status: EnhancementStatus::MenuOpen,
        })
    );
}

#[test]
fn template_route_classifies_servant_enhancement_variants() {
    assert_eq!(
        classify_enhancement_route(&ProbeSnapshot::from_keys(&[
            "text_enhancement_servant",
            "text_enhancement_result"
        ])),
        Some(EnhancementRoute {
            screen: EnhancementTopScreen::ServantEnhancement,
            variant: EnhancementVariant::Main,
            status: EnhancementStatus::None,
        })
    );
    assert_eq!(
        classify_enhancement_route(&ProbeSnapshot::from_keys(&[
            "text_enhancement_servant",
            "text_enhancement_servant_select"
        ])),
        Some(EnhancementRoute {
            screen: EnhancementTopScreen::ServantEnhancement,
            variant: EnhancementVariant::ServantSelect,
            status: EnhancementStatus::None,
        })
    );
    assert_eq!(
        classify_enhancement_route(&ProbeSnapshot::from_keys(&[
            "text_enhancement_servant",
            "text_enhancement_material"
        ])),
        Some(EnhancementRoute {
            screen: EnhancementTopScreen::ServantEnhancement,
            variant: EnhancementVariant::MaterialSelect,
            status: EnhancementStatus::None,
        })
    );
    assert_eq!(
        classify_enhancement_route(&ProbeSnapshot::from_keys(&[
            "text_enhancement_servant",
            "text_enhancement_material",
            "dialog_filter_setting"
        ])),
        Some(EnhancementRoute {
            screen: EnhancementTopScreen::ServantEnhancement,
            variant: EnhancementVariant::MaterialSelect,
            status: EnhancementStatus::FilterDialogOpen,
        })
    );
}

#[test]
fn template_route_classifies_ascension_variants() {
    assert_eq!(
        classify_enhancement_route(&ProbeSnapshot::from_keys(&[
            "screen_enhancement_ascension",
            "text_ascension_main_variant"
        ])),
        Some(EnhancementRoute {
            screen: EnhancementTopScreen::Ascension,
            variant: EnhancementVariant::Main,
            status: EnhancementStatus::None,
        })
    );
    assert_eq!(
        classify_enhancement_route(&ProbeSnapshot::from_keys(&[
            "screen_enhancement_ascension",
            "text_enhancement_ascension_servant_select"
        ])),
        Some(EnhancementRoute {
            screen: EnhancementTopScreen::Ascension,
            variant: EnhancementVariant::ServantSelect,
            status: EnhancementStatus::None,
        })
    );
    assert_eq!(
        classify_enhancement_route(&ProbeSnapshot::from_keys(&[
            "screen_enhancement_ascension",
            "enhancement_ascension_not_ready"
        ])),
        Some(EnhancementRoute {
            screen: EnhancementTopScreen::Ascension,
            variant: EnhancementVariant::Main,
            status: EnhancementStatus::NotReady,
        })
    );
}

#[test]
fn template_route_prefers_specific_enhancement_screen_over_generic_title() {
    assert_eq!(
        classify_enhancement_route(&ProbeSnapshot::from_keys(&[
            "text_enhancement",
            "text_enhancement_servant",
            "text_enhancement_result"
        ])),
        Some(EnhancementRoute {
            screen: EnhancementTopScreen::ServantEnhancement,
            variant: EnhancementVariant::Main,
            status: EnhancementStatus::None,
        })
    );
}

#[test]
fn scale_level_3_retries_until_match_or_three_taps() {
    assert_eq!(scale_level_3_decision(0, true), ScaleLevel3Decision::Done);
    assert_eq!(
        scale_level_3_decision(0, false),
        ScaleLevel3Decision::TapAndRetry
    );
    assert_eq!(
        scale_level_3_decision(2, false),
        ScaleLevel3Decision::TapAndRetry
    );
    assert_eq!(scale_level_3_decision(3, false), ScaleLevel3Decision::Fail);
}
