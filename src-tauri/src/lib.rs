use std::sync::OnceLock;

#[derive(serde::Serialize, Clone)]
struct ServantInfo {
    id: u32,
    name_cn: String,
    name_jp: String,
    name_en: String,
    name_other: Option<String>,
    class: String,
    rarity: u32,
}

fn servants_data() -> &'static [ServantInfo] {
    static SERVANTS: OnceLock<Vec<ServantInfo>> = OnceLock::new();
    SERVANTS.get_or_init(|| {
        let raw: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("resources/servants.json"))
                .expect("invalid servants.json");

        raw.iter()
            .enumerate()
            .filter_map(|(idx, s)| {
                let result = (|| {
                    let id = s.get("id")?.as_u64()? as u32;
                    let name_cn = s.get("name_cn")?.as_str()?.to_string();
                    let name_jp = s.get("name_jp")?.as_str()?.to_string();
                    let name_en = s.get("name_en")?.as_str()?.to_string();
                    let name_other = s
                        .get("name_other")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let base_stats = s.get("base_stats")?.as_object()?;

                    let (class, rarity) = if let Some(cls) = base_stats.get("class") {
                        let class = cls.as_str()?.to_string();
                        let rarity = base_stats.get("rarity")?.as_u64()? as u32;
                        (class, rarity)
                    } else {
                        let first_variant = base_stats.values().next()?.as_object()?;
                        let class = first_variant.get("class")?.as_str()?.to_string();
                        let rarity = first_variant.get("rarity")?.as_u64()? as u32;
                        (class, rarity)
                    };

                    Some(ServantInfo {
                        id,
                        name_cn,
                        name_jp,
                        name_en,
                        name_other,
                        class,
                        rarity,
                    })
                })();

                if result.is_none() {
                    let id_hint = s.get("id").and_then(|v| v.as_u64());
                    eprintln!(
                        "[servants] dropping entry at index {idx} (id={id_hint:?}): missing or invalid fields"
                    );
                }
                result
            })
            .collect()
    })
}

#[tauri::command]
fn get_servants() -> &'static [ServantInfo] {
    servants_data()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![get_servants])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
