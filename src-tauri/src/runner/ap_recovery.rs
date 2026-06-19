//! AP recovery item mapping and page candidate ordering.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApRecoveryPage {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ApRecoveryTemplate {
    pub(crate) item: ApRecoveryItem,
    pub(crate) page: ApRecoveryPage,
    pub(crate) label: &'static str,
    pub(crate) template_key: &'static str,
}

pub(crate) fn ap_recovery_template(item: ApRecoveryItem) -> ApRecoveryTemplate {
    match item {
        // Frontend `Rainbow` maps to the premium Saint Quartz recovery option.
        ApRecoveryItem::Rainbow => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Top,
            label: "圣晶石",
            template_key: "items/item_saint_quartz",
        },
        ApRecoveryItem::Gold => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Top,
            label: "黄金苹果",
            template_key: "items/item_apple_gold",
        },
        ApRecoveryItem::Silver => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Top,
            label: "白银苹果",
            template_key: "items/item_apple_silver",
        },
        ApRecoveryItem::Bronze => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Bottom,
            label: "青铜苹果",
            template_key: "items/item_apple_bronzed_cobalt",
        },
        ApRecoveryItem::Copper => ApRecoveryTemplate {
            item,
            page: ApRecoveryPage::Bottom,
            label: "赤铜苹果",
            template_key: "items/item_apple_bronze",
        },
    }
}

pub(crate) fn ap_recovery_candidates_for_page(
    configured: &[ApRecoveryItem],
    page: ApRecoveryPage,
) -> Vec<ApRecoveryTemplate> {
    [
        ApRecoveryItem::Gold,
        ApRecoveryItem::Silver,
        ApRecoveryItem::Bronze,
        ApRecoveryItem::Copper,
        ApRecoveryItem::Rainbow,
    ]
    .into_iter()
    .filter(|item| configured.contains(item))
    .map(ap_recovery_template)
    .filter(|template| template.page == page)
    .collect()
}
