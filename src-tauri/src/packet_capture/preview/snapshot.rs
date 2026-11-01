use super::*;
use serde_json::Value;
use std::mem::size_of;

const TABLES: [(&str, &str); 10] = [
    ("userGame", "userId"),
    ("tblUserGame", "userId"),
    ("userItem", "itemId"),
    ("userSvt", "id"),
    ("userSvtStorage", "id"),
    ("userSvtCollection", "svtId"),
    ("userSvtCoin", "svtId"),
    ("userSvtAppendPassiveSkillLv", "userSvtId"),
    ("userSvtAppendPassiveSkill", "svtId"),
    ("userPresentBox", "presentId"),
];
type Tables = BTreeMap<String, BTreeMap<String, Value>>;

pub(super) fn num(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.parse().ok())
}
fn text(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::to_string)
        .or_else(|| num(v).map(|n| n.to_string()))
}
fn values(v: &Value) -> Vec<u64> {
    v.as_array().into_iter().flatten().filter_map(num).collect()
}

#[derive(Default)]
pub(super) struct Instance {
    pub id: String,
    pub svt_id: u64,
    pub level: Option<u64>,
    pub exp: Option<u64>,
    pub limit_count: Option<u64>,
    pub np_level: Option<u64>,
    pub skills: [Option<u64>; 3],
    pub hp: Option<u64>,
    pub atk: Option<u64>,
    pub fou_hp: Option<u64>,
    pub fou_atk: Option<u64>,
    pub locked: Option<bool>,
    pub in_storage: bool,
}
#[derive(Default)]
pub(super) struct Collection {
    pub bond: Option<u64>,
    pub bond_points: Option<u64>,
    pub bond_exceed: Option<u64>,
    pub costumes: Vec<u64>,
}
#[derive(Default)]
pub(super) struct Append {
    pub levels: BTreeMap<u64, Option<u64>>,
}

pub(crate) struct Snapshot {
    pub(super) profile: Vec<PreviewField>,
    pub(super) currencies: Vec<PreviewItem>,
    pub(super) items: BTreeMap<u64, u64>,
    pub(super) instances: Vec<Instance>,
    pub(super) collections: BTreeMap<u64, Collection>,
    pub(super) coins: BTreeMap<u64, u64>,
    pub(super) appends: BTreeMap<String, Append>,
    pub(super) unlocks: BTreeMap<u64, Vec<u64>>,
    pub(super) has_inventory: bool,
    pub(super) has_storage: bool,
    pub(super) presents: Option<usize>,
}

impl Snapshot {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        let json = super::super::response::decoded_json(raw).ok_or("无法解析此记录的登录响应")?;
        let mut tables = Tables::new();
        collect(&json, &mut tables, 0);
        if tables.is_empty() {
            return Err("此响应中没有可预览的账户数据".into());
        }
        let rows = |name: &str| -> Vec<&Value> {
            tables
                .get(name)
                .into_iter()
                .flat_map(|v| v.values())
                .collect()
        };
        let game = rows("userGame")
            .first()
            .copied()
            .cloned()
            .unwrap_or(Value::Null);
        let tbl_game = rows("tblUserGame")
            .first()
            .copied()
            .cloned()
            .unwrap_or(Value::Null);
        let items: BTreeMap<u64, u64> = rows("userItem")
            .iter()
            .filter_map(|v| Some((num(&v["itemId"])?, num(&v["num"])?)))
            .collect();
        let mut currencies = Vec::new();
        for (id, key, name, item_id) in [
            ("qp", "qp", "QP", 1),
            ("stone", "stone", "圣晶石", 2),
            ("mana", "mana", "魔力棱镜", 0),
            ("rarePri", "rarePri", "稀有魔力棱镜", 0),
        ] {
            if let Some(quantity) = num(&game[key]).or_else(|| {
                (item_id != 0)
                    .then(|| items.get(&item_id).copied())
                    .flatten()
            }) {
                let note = if key == "stone" {
                    let free = num(&game["freeStone"]).map_or("—".into(), |v| v.to_string());
                    let paid = num(&game["chargeStone"]).map_or("—".into(), |v| v.to_string());
                    Some(format!("免费 {free} · 付费 {paid}"))
                } else {
                    None
                };
                currencies.push(PreviewItem {
                    id: format!("currency:{id}"),
                    item_id,
                    name: name.into(),
                    quantity,
                    category: "currency".into(),
                    icon: None,
                    note,
                });
            }
        }
        if let Some(quantity) = num(&tbl_game["friendPoint"]) {
            currencies.insert(
                currencies.len().min(2),
                PreviewItem {
                    id: "currency:friendPoint".into(),
                    item_id: 0,
                    name: "友情点".into(),
                    quantity,
                    category: "currency".into(),
                    icon: None,
                    note: None,
                },
            );
        }
        let mut instances = Vec::new();
        for (table, in_storage) in [("userSvt", false), ("userSvtStorage", true)] {
            for v in rows(table) {
                let (Some(id), Some(svt_id)) = (text(&v["id"]), num(&v["svtId"])) else {
                    continue;
                };
                instances.push(Instance {
                    id,
                    svt_id,
                    in_storage,
                    level: num(&v["lv"]),
                    exp: num(&v["exp"]),
                    limit_count: num(&v["limitCount"]),
                    np_level: num(&v[format!(
                        "treasureDeviceLv{}",
                        num(&v["selectTreasureDeviceIdx"])
                            .filter(|n| *n < 3)
                            .unwrap_or(0)
                            + 1
                    )]),
                    skills: [
                        num(&v["skillLv1"]),
                        num(&v["skillLv2"]),
                        num(&v["skillLv3"]),
                    ],
                    hp: num(&v["hp"]),
                    atk: num(&v["atk"]),
                    fou_hp: num(&v["adjustHp"]),
                    fou_atk: num(&v["adjustAtk"]),
                    locked: num(&v["isLock"]).map(|n| n != 0),
                });
            }
        }
        // A moved instance must never be counted twice if a response contains both tables.
        instances.sort_by(|a, b| a.id.cmp(&b.id).then(b.in_storage.cmp(&a.in_storage)));
        instances.dedup_by(|a, b| a.id == b.id);
        Ok(Self {
            profile: vec![
                field("御主", text(&game["name"])),
                field("等级", num(&game["lv"])),
                field("好友码", text(&game["friendCode"])),
                field("签名", text(&game["message"])),
                field("经验", num(&game["exp"])),
                field("COST 上限", num(&game["costMax"])),
            ],
            currencies,
            items,
            instances,
            collections: rows("userSvtCollection")
                .iter()
                .filter_map(|v| {
                    Some((
                        num(&v["svtId"])?,
                        Collection {
                            bond: num(&v["friendshipRank"]),
                            bond_points: num(&v["friendship"]),
                            bond_exceed: num(&v["friendshipExceedCount"]),
                            costumes: values(&v["costumeIds"]),
                        },
                    ))
                })
                .collect(),
            coins: rows("userSvtCoin")
                .iter()
                .filter_map(|v| Some((num(&v["svtId"])?, num(&v["num"])?)))
                .collect(),
            appends: rows("userSvtAppendPassiveSkillLv")
                .iter()
                .filter_map(|v| {
                    Some((
                        text(&v["userSvtId"])?,
                        Append {
                            levels: v["appendPassiveSkillNums"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .enumerate()
                                .filter_map(|(index, slot)| {
                                    Some((
                                        num(slot)?,
                                        v["appendPassiveSkillLvs"].get(index).and_then(num),
                                    ))
                                })
                                .collect(),
                        },
                    ))
                })
                .collect(),
            unlocks: rows("userSvtAppendPassiveSkill")
                .iter()
                .filter_map(|v| Some((num(&v["svtId"])?, values(&v["unlockNums"]))))
                .collect(),
            has_inventory: tables.contains_key("userSvt"),
            has_storage: tables.contains_key("userSvtStorage"),
            presents: tables.get("userPresentBox").map(BTreeMap::len),
        })
    }

    pub(crate) fn allocated_bytes(&self) -> usize {
        // Conservatively include tree-node overhead as well as owned buffers.
        size_of::<Self>()
            + self.instances.capacity() * size_of::<Instance>()
            + self
                .instances
                .iter()
                .map(|v| v.id.capacity())
                .sum::<usize>()
            + self
                .profile
                .iter()
                .map(|v| {
                    size_of::<PreviewField>()
                        + v.label.capacity()
                        + v.value.as_ref().map_or(0, String::capacity)
                })
                .sum::<usize>()
            + self
                .currencies
                .iter()
                .map(|v| {
                    size_of::<PreviewItem>()
                        + v.id.capacity()
                        + v.name.capacity()
                        + v.category.capacity()
                        + v.note.as_ref().map_or(0, String::capacity)
                })
                .sum::<usize>()
            + self.items.len() * 96
            + self.coins.len() * 96
            + self
                .collections
                .values()
                .map(|v| 128 + v.costumes.capacity() * size_of::<u64>())
                .sum::<usize>()
            + self
                .appends
                .iter()
                .map(|(k, v)| 128 + k.capacity() + v.levels.len() * 96)
                .sum::<usize>()
            + self
                .unlocks
                .values()
                .map(|v| 96 + v.capacity() * size_of::<u64>())
                .sum::<usize>()
    }
}

fn apply(value: &Value, tables: &mut Tables, mode: &str) {
    let Some(object) = value.as_object() else {
        return;
    };
    for (name, key) in TABLES {
        let Some(rows) = object.get(name).and_then(Value::as_array) else {
            continue;
        };
        let table = tables.entry(name.into()).or_default();
        if mode == "replaced" {
            table.clear();
        }
        for row in rows {
            let Some(id) = text(&row[key])
                .or_else(|| (name == "userGame").then(|| text(&row["id"])).flatten())
            else {
                continue;
            };
            if mode == "deleted" {
                table.remove(&id);
                continue;
            }
            if let Some(existing) = table.get_mut(&id).and_then(Value::as_object_mut) {
                if let Some(fields) = row.as_object() {
                    existing.extend(fields.clone());
                }
            } else {
                table.insert(id, row.clone());
            }
        }
    }
}

fn collect(value: &Value, tables: &mut Tables, depth: usize) {
    if depth > 10 {
        return;
    }
    match value {
        Value::Object(object) => {
            apply(value, tables, "updated");
            for key in ["response", "success", "data", "cache"] {
                if let Some(child) = object.get(key) {
                    collect(child, tables, depth + 1);
                }
            }
            for mode in ["replaced", "updated", "deleted"] {
                if let Some(child) = object.get(mode) {
                    apply(child, tables, mode);
                }
            }
        }
        Value::Array(rows) => {
            for row in rows {
                collect(row, tables, depth + 1);
            }
        }
        _ => {}
    }
}
