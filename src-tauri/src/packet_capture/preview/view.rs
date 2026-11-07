use super::*;

pub(super) fn build(
    s: &Snapshot,
    record_id: &str,
    q: PreviewQuery,
    catalog: &CaptureCatalog,
    assets: &Path,
) -> CapturePreview {
    let mut servants = Vec::new();
    let mut ces = BTreeMap::<u64, PreviewCe>::new();
    let mut other = BTreeMap::<u64, u64>::new();
    let mut classes = BTreeMap::new();
    for row in &s.instances {
        if let Some(info) = catalog.servants.get(&row.svt_id) {
            let collection = s.collections.get(&row.svt_id);
            if let (Some(id), Some(name)) = (info.class_id, info.class_name.as_deref()) {
                classes.insert(id, class_name(name));
            }
            let mut details = vec![
                field("实例 ID", Some(&row.id)),
                field("从者 ID", Some(row.svt_id)),
                field("经验", row.exp),
                field("灵基再临", row.limit_count),
                field("HP", row.hp),
                field("ATK", row.atk),
                field("芙芙 HP", row.fou_hp),
                field("芙芙 ATK", row.fou_atk),
                field(
                    "羁绊值（同种从者共享）",
                    collection.and_then(|c| c.bond_points),
                ),
                field("羁绊上限突破次数", collection.and_then(|c| c.bond_exceed)),
                field("从者硬币（同种从者共享）", s.coins.get(&row.svt_id)),
                field("锁定", row.locked.map(|v| if v { "是" } else { "否" })),
            ];
            let append = s.appends.get(&row.id);
            for index in 0..5 {
                let slot = 100 + index;
                let level = append.and_then(|a| a.levels.get(&slot)).copied().flatten();
                details.push(field(
                    &format!("追加技能 {}", index + 1),
                    level.map(|n| format!("Lv.{n}")),
                ));
            }
            if let Some(unlocks) = s.unlocks.get(&row.svt_id) {
                details.push(field(
                    "已解锁追加槽位（同种从者共享）",
                    Some(
                        unlocks
                            .iter()
                            .map(|n| {
                                n.checked_sub(99)
                                    .map_or_else(|| n.to_string(), |v| v.to_string())
                            })
                            .collect::<Vec<_>>()
                            .join(" / "),
                    ),
                ));
            }
            details.push(field(
                "灵衣 ID",
                collection.map(|c| {
                    if c.costumes.is_empty() {
                        "无".into()
                    } else {
                        c.costumes
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("、")
                    }
                }),
            ));
            servants.push(PreviewServant {
                instance_id: row.id.clone(),
                svt_id: row.svt_id,
                number: Some(info.number),
                name: info.name.clone(),
                rarity: info.rarity,
                class_id: info.class_id,
                class_name: info.class_name.as_deref().map(class_name),
                icon: None,
                level: row.level,
                np_level: row.np_level,
                skills: row.skills,
                bond: collection.and_then(|c| c.bond),
                in_storage: row.in_storage,
                details,
            });
        } else if let Some(info) = catalog.ces.get(&row.svt_id) {
            let ce = ces.entry(row.svt_id).or_insert_with(|| PreviewCe {
                svt_id: row.svt_id,
                number: info.number,
                name: info.name.clone(),
                rarity: info.rarity,
                icon: None,
                quantity: 0,
                summary: String::new(),
                instances: Vec::new(),
            });
            ce.instances.push(PreviewCeInstance {
                instance_id: row.id.clone(),
                level: row.level,
                limit_count: row.limit_count,
                limit_broken: row.limit_count.map(|n| n >= 4),
                in_storage: row.in_storage,
                locked: row.locked,
            });
        } else {
            *other.entry(row.svt_id).or_default() += 1;
        }
    }
    let known_inventory = s.has_inventory && s.has_storage;
    let summary = PreviewSummary {
        servant_instances: known_inventory.then_some(servants.len()),
        servant_types: known_inventory.then(|| {
            servants
                .iter()
                .map(|v| v.svt_id)
                .collect::<BTreeSet<_>>()
                .len()
        }),
        ce_instances: known_inventory.then(|| ces.values().map(|v| v.instances.len()).sum()),
        ce_types: known_inventory.then_some(ces.len()),
        storage_instances: s
            .has_storage
            .then(|| s.instances.iter().filter(|v| v.in_storage).count()),
        unclassified_instances: known_inventory.then(|| other.values().sum::<u64>() as usize),
        presents: s.presents,
    };
    let search = q.search.trim().to_lowercase();
    let matches = |name: &str, number: u64| {
        name.to_lowercase().contains(&search) || number.to_string().contains(&search)
    };
    servants.retain(|v| {
        matches(&v.name, v.number.unwrap_or(v.svt_id))
            && q.class_id.is_none_or(|id| v.class_id == Some(id))
            && q.rarity.is_none_or(|n| v.rarity == Some(n))
            && storage_matches(q.storage, v.in_storage)
    });
    servants.sort_by(|a, b| {
        let order = match q.sort {
            PreviewSort::Level => b.level.cmp(&a.level),
            PreviewSort::Bond => b.bond.cmp(&a.bond),
            _ => std::cmp::Ordering::Equal,
        };
        order
            .then(a.number.cmp(&b.number))
            .then(a.instance_id.cmp(&b.instance_id))
    });
    let mut ces: Vec<_> = ces
        .into_values()
        .filter_map(|mut ce| {
            if !matches(&ce.name, ce.number) || q.rarity.is_some_and(|n| ce.rarity != Some(n)) {
                return None;
            }
            ce.instances.retain(|v| {
                storage_matches(q.storage, v.in_storage)
                    && (!q.limit_broken || v.limit_broken == Some(true))
            });
            if ce.instances.is_empty() {
                return None;
            }
            ce.instances.sort_by(|a, b| {
                b.level
                    .cmp(&a.level)
                    .then(b.limit_count.cmp(&a.limit_count))
                    .then(a.instance_id.cmp(&b.instance_id))
            });
            ce.quantity = ce.instances.len();
            let mut groups = BTreeMap::<(Option<u64>, Option<u64>, bool), usize>::new();
            for v in &ce.instances {
                *groups
                    .entry((v.level, v.limit_count, v.in_storage))
                    .or_default() += 1;
            }
            ce.summary = groups
                .into_iter()
                .rev()
                .map(|((level, limit, storage), count)| {
                    let level = level.map_or("—".into(), |v| v.to_string());
                    let limit = limit.map_or("突破 —".into(), |v| {
                        if v >= 4 {
                            "满破".into()
                        } else {
                            format!("突破 {v}")
                        }
                    });
                    format!(
                        "Lv.{level} {limit}{} ×{count}",
                        if storage { " · 保管室" } else { "" }
                    )
                })
                .collect::<Vec<_>>()
                .join("；");
            Some(ce)
        })
        .collect();
    ces.sort_by(|a, b| {
        let order = match q.sort {
            PreviewSort::Level => b
                .instances
                .first()
                .and_then(|v| v.level)
                .cmp(&a.instances.first().and_then(|v| v.level)),
            PreviewSort::Quantity => b.quantity.cmp(&a.quantity),
            _ => std::cmp::Ordering::Equal,
        };
        order.then(a.number.cmp(&b.number))
    });
    let mut items: Vec<_> = s
        .items
        .iter()
        .map(|(id, quantity)| {
            let info = catalog.items.get(id);
            let common_name = match id {
                1 => Some("QP"),
                2 => Some("圣晶石"),
                100 => Some("黄金果实"),
                101 => Some("白银果实"),
                102 => Some("赤铜果实"),
                104 => Some("青铜果实"),
                _ => None,
            };
            PreviewItem {
                id: format!("item:{id}"),
                item_id: *id,
                name: common_name
                    .map(str::to_string)
                    .or_else(|| info.map(|v| v.name.clone()))
                    .unwrap_or_else(|| format!("物品 #{id}")),
                quantity: *quantity,
                category: if [100, 101, 102, 104].contains(id) {
                    "consumable"
                } else if info.is_some() {
                    "event"
                } else {
                    "other"
                }
                .into(),
                icon: info
                    .and_then(|v| v.icon.as_deref())
                    .and_then(|p| icon_at(assets, p)),
                note: None,
            }
        })
        .collect();
    items.extend(other.into_iter().map(|(id, quantity)| PreviewItem {
        id: format!("instance:{id}"),
        item_id: id,
        name: format!("未识别实例 #{id}"),
        quantity,
        category: "other".into(),
        icon: None,
        note: Some("按实例计数".into()),
    }));
    let fruits = [100, 101, 104, 102]
        .into_iter()
        .filter_map(|id| items.iter().find(|v| v.id == format!("item:{id}")).cloned())
        .collect();
    items.retain(|v| {
        (q.show_zero || v.quantity != 0)
            && matches(&v.name, v.item_id)
            && q.item_category
                .as_deref()
                .is_none_or(|category| v.category == category)
    });
    items.sort_by(|a, b| {
        let order = if q.sort == PreviewSort::Quantity {
            b.quantity.cmp(&a.quantity)
        } else {
            std::cmp::Ordering::Equal
        };
        order.then(a.item_id.cmp(&b.item_id)).then(a.id.cmp(&b.id))
    });
    let total = match q.tab {
        PreviewTab::Overview => 0,
        PreviewTab::Servants => servants.len(),
        PreviewTab::CraftEssences => ces.len(),
        PreviewTab::Items => items.len(),
    };
    let page_count = total.div_ceil(40).max(1);
    let page = q.page.min(page_count - 1);
    let start = page * 40;
    let mut servants: Vec<_> = if q.tab == PreviewTab::Servants {
        servants.into_iter().skip(start).take(40).collect()
    } else {
        Vec::new()
    };
    let mut ces: Vec<_> = if q.tab == PreviewTab::CraftEssences {
        ces.into_iter().skip(start).take(40).collect()
    } else {
        Vec::new()
    };
    for v in &mut servants {
        v.icon = crate::commands::catalog::pick_face_in(
            &assets
                .join("servants")
                .join(v.number.unwrap_or(v.svt_id).to_string()),
        )
        .map(|p| p.to_string_lossy().into_owned());
    }
    for v in &mut ces {
        v.icon = icon_at(assets, &format!("ces/{}/card_ce.png", v.number));
    }
    CapturePreview {
        record_id: record_id.into(),
        tab: q.tab,
        profile: s.profile.clone(),
        currencies: s.currencies.clone(),
        fruits,
        summary,
        classes: classes
            .into_iter()
            .map(|(id, name)| PreviewClass { id, name })
            .collect(),
        servants,
        craft_essences: ces,
        items: if q.tab == PreviewTab::Items {
            items.into_iter().skip(start).take(40).collect()
        } else {
            Vec::new()
        },
        total,
        page,
        page_count,
    }
}

fn class_name(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "saber" => "剑士",
        "archer" => "弓兵",
        "lancer" => "枪兵",
        "rider" => "骑兵",
        "caster" => "术师",
        "assassin" => "暗匿者",
        "berserker" => "狂战士",
        "shielder" => "盾兵",
        "ruler" => "裁定者",
        "avenger" => "复仇者",
        "mooncancer" => "月之癌",
        "alterego" => "他人格",
        "foreigner" => "降临者",
        "pretender" => "伪装者",
        "beast" => "兽",
        _ => name,
    }
    .into()
}
