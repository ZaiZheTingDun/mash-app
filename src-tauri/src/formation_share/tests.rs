use super::*;
use serde_json::json;

fn entry() -> Value {
    json!({
        "project":{
            "id":"private-project-id", "name":"双同从者", "advancedMode":false,
            "supportServantId":309, "repeatMission":true, "repeatMode":"infinite",
            "apRecoveryItems":["rainbow"], "server":"jp",
            "recognitionSettings":{"supportClassFilterMode":"allClasses","supportCeThreshold":0.8,"stopOnFiveStarCeDrop":true},
            "slots":[
                {"id":"owned","type":"servant","servantId":309,"craftEssenceId":100},
                {"id":"two","type":"servant","servantId":284,"craftEssenceId":200},
                {"id":"borrowed","type":"support","servantId":null,"craftEssenceIds":[300,400],"craftEssenceMultiSelect":true,"craftEssenceMlbRequired":false},
                {"id":"four","type":"servant","servantId":null},
                {"id":"five","type":"servant","servantId":null},
                {"id":"six","type":"servant","servantId":null}
            ]
        },
        "battleScenes":[{"id":"wave-1","turns":[{"id":"turn-1","preparationActions":[
            {"id":"skill-1","type":"servant","servant":"servant_3","servantMemberId":"borrowed","servantId":309,"servantIsSupport":true,"skill":"skill_1","target":"servant_1","targetMemberId":"owned","targetServantId":309,"targetIsSupport":false},
            {"id":"swap","type":"equipment","skill":"skill_3","orderChange":{"front":"servant_1","frontMemberId":"owned","back":"servant_4","backMemberId":"four"}}
        ],"attackPriority":[{"id":"np","card":"servant_1_np","memberId":"owned","servantId":309,"isSupport":false}]}]}],
        "advancedBattleScenes":[]
    })
}

fn packet(json: &[u8]) -> String {
    let mut gzip = GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), Compression::best());
    gzip.write_all(json).unwrap();
    let mut bytes = b"MASH\x01\x01".to_vec();
    bytes.extend_from_slice(&(json.len() as u32).to_be_bytes());
    bytes.extend(gzip.finish().unwrap());
    STANDARD.encode(bytes)
}

fn envelope(code: &str) -> Value {
    let bytes = STANDARD.decode(code).unwrap();
    let mut json = Vec::new();
    GzDecoder::new(&bytes[10..]).read_to_end(&mut json).unwrap();
    serde_json::from_slice(&json).unwrap()
}

#[test]
fn round_trip_preserves_owned_support_targets_order_change_and_ce_preferences() {
    let code = encode(&entry()).unwrap();
    assert_eq!(code, encode(&entry()).unwrap());
    let decoded = decode(&code).unwrap();
    let slots = &decoded["project"]["slots"];
    let actions = &decoded["battleScenes"][0]["turns"][0]["preparationActions"];
    assert_eq!(actions[0]["servantMemberId"], slots[2]["id"]);
    assert_eq!(actions[0]["targetMemberId"], slots[0]["id"]);
    assert_ne!(slots[0]["id"], slots[2]["id"]);
    assert_eq!(actions[1]["orderChange"]["frontMemberId"], slots[0]["id"]);
    assert_eq!(actions[1]["orderChange"]["backMemberId"], slots[3]["id"]);
    assert_eq!(slots[2]["craftEssenceIds"], json!([300, 400]));
    assert_eq!(slots[2]["craftEssenceMlbRequired"], false);
    assert!(decoded["project"].get("recognitionSettings").is_none());
    let serialized = decoded.to_string();
    for excluded in [
        "private-project-id",
        "repeatMode",
        "repeatMission",
        "apRecovery",
        "server",
        "recognitionSettings",
        "supportClassFilterMode",
        "supportCeThreshold",
        "stopOnFiveStarCeDrop",
    ] {
        assert!(!serialized.contains(excluded), "leaked {excluded}");
    }
}

#[test]
fn whitespace_is_allowed_but_truncation_and_wrong_base64_are_not() {
    let code = encode(&entry()).unwrap();
    let wrapped = code
        .as_bytes()
        .chunks(40)
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect::<Vec<_>>()
        .join("\r\n");
    assert_eq!(
        decode(&format!(" \n{wrapped}\t")).unwrap(),
        decode(&code).unwrap()
    );
    assert!(decode(&code[..code.len() - 5]).is_err());
    assert!(decode("这不是分享码").is_err());
    assert!(decode(&"A".repeat(MAX_TEXT_BYTES + 1)).is_err());
}

#[test]
fn future_formats_features_and_fields_are_never_silently_ignored() {
    let mut input = entry();
    input["project"]["advancedMode"] = json!(true);
    let code = encode(&input).unwrap();
    let mut bytes = STANDARD.decode(&code).unwrap();
    bytes[4] = 2;
    assert!(decode(&STANDARD.encode(bytes))
        .unwrap_err()
        .contains("升级"));
    for (key, value) in [("v", json!(2)), ("f", json!([99]))] {
        let mut value_json = envelope(&code);
        value_json[key] = value;
        assert!(decode(&packet(&serde_json::to_vec(&value_json).unwrap()))
            .unwrap_err()
            .contains("升级"));
    }
    let mut value = envelope(&code);
    value["c"]["future"] = json!(true);
    assert!(decode(&packet(&serde_json::to_vec(&value).unwrap())).is_err());
    let mut value = envelope(&code);
    value["f"] = json!([]);
    assert!(decode(&packet(&serde_json::to_vec(&value).unwrap()))
        .unwrap_err()
        .contains("功能声明"));
}

#[test]
fn corruption_trailing_data_and_false_lengths_are_rejected() {
    let code = encode(&entry()).unwrap();
    let bytes = STANDARD.decode(code).unwrap();
    let mut corrupt = bytes.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(decode(&STANDARD.encode(corrupt)).is_err());
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(decode(&STANDARD.encode(extra)).is_err());
    let mut length = bytes;
    length[9] ^= 1;
    assert!(decode(&STANDARD.encode(length)).is_err());
}

#[test]
fn bounded_decompression_checks_actual_output_not_just_the_header() {
    let code = packet(&vec![b' '; MAX_JSON_BYTES + 1]);
    let mut bytes = STANDARD.decode(code).unwrap();
    bytes[6..10].copy_from_slice(&1_u32.to_be_bytes());
    assert!(decode(&STANDARD.encode(bytes)).is_err());
}

#[test]
fn duplicate_keys_dangling_references_and_duplicate_members_are_rejected() {
    assert!(decode(&packet(br#"{"v":1,"v":2,"f":[],"c":{}}"#))
        .unwrap_err()
        .contains("重复字段"));
    let mut input = entry();
    input["battleScenes"][0]["turns"][0]["preparationActions"][0]["targetMemberId"] =
        json!("missing");
    assert!(encode(&input).unwrap_err().contains("引用不存在"));
    let mut input = entry();
    input["project"]["slots"][1]["id"] = json!("owned");
    assert!(encode(&input).unwrap_err().contains("重复"));
}

#[test]
fn advanced_conditions_and_strategy_member_references_round_trip() {
    let mut input = entry();
    input["project"]["advancedMode"] = json!(true);
    input["project"]["grandServants"] = json!([{"memberId":"borrowed","slotIndex":2,"servantId":309,"isSupport":true,"npCard":"arts","priority":"np","role":"main"}]);
    input["advancedBattleScenes"] = json!([{"id":"advanced","rules":[{"id":"rule","npConditionGroups":[{"id":"condition","slots":[{"servant":"servant_3","memberId":"borrowed","servantId":309,"isSupport":true,"ready":false}]}],"actions":[{"id":"attack","type":"attack","memberId":"borrowed","servantId":309,"isSupport":true,"card":"servant_3_np"}]}]}]);
    let decoded = decode(&encode(&input).unwrap()).unwrap();
    let support = &decoded["project"]["slots"][2]["id"];
    assert_eq!(&decoded["project"]["grandServants"][0]["memberId"], support);
    assert_eq!(
        &decoded["advancedBattleScenes"][0]["rules"][0]["actions"][0]["memberId"],
        support
    );
    assert_eq!(
        decoded["advancedBattleScenes"][0]["rules"][0]["npConditionGroups"][0]["slots"][0]["ready"],
        false
    );
}

#[test]
fn decoder_cannot_ignore_fields_discarded_by_application_models() {
    assert!(
        reject_discarded_fields(&json!({"skill":"x","future":true}), &json!({"skill":"x"}))
            .is_err()
    );
}

#[test]
fn fixed_v1_fixture_contains_only_supported_formation_data() {
    let decoded = decode(include_str!("fixtures/v1.txt")).unwrap();
    assert_eq!(decoded["project"]["name"], "v1 固定兼容样例");
    assert_eq!(decode(&encode(&decoded).unwrap()).unwrap(), decoded);
    for local in [
        "recognitionSettings",
        "supportClassFilterMode",
        "repeatMission",
        "repeatMode",
        "repeatCount",
        "apRecoveryItems",
        "apRecoveryLimits",
    ] {
        assert!(
            decoded["project"].get(local).is_none(),
            "local setting in fixture: {local}"
        );
    }
    assert_eq!(
        decoded["battleScenes"][0]["turns"][0]["preparationActions"][0]["servantMemberId"],
        decoded["project"]["slots"][2]["id"]
    );
}

#[test]
fn informational_extensions_do_not_change_execution_data() {
    let code = encode(&entry()).unwrap();
    let mut extended = envelope(&code);
    extended["x"] = json!({"futureDisplayNote":"仅用于展示的注记"});
    assert_eq!(
        decode(&packet(&serde_json::to_vec(&extended).unwrap())).unwrap(),
        decode(&code).unwrap()
    );
}

#[test]
fn new_application_fields_require_an_explicit_portability_decision() {
    let mut value = entry();
    value["project"]["futureTacticalOption"] = json!(true);
    assert!(encode(&value).unwrap_err().contains("尚未支持项目字段"));
}

#[test]
fn malformed_scene_and_turn_containers_return_errors_instead_of_panicking() {
    let code = encode(&entry()).unwrap();
    for scenes in [
        json!([true]),
        json!([null]),
        json!([{"i":7,"ts":[false]}]),
        json!(true),
    ] {
        let mut input = entry();
        input["battleScenes"] = scenes.clone();
        assert!(encode(&input).is_err());
        let mut input = envelope(&code);
        input["c"]["b"] = scenes;
        assert!(decode(&packet(&serde_json::to_vec(&input).unwrap())).is_err());
    }
}

#[test]
fn local_support_class_and_run_preferences_never_change_the_shared_code() {
    let original = entry();
    let expected = encode(&original).unwrap();
    for mode in ["correspondingClass", "allClasses", "both"] {
        let mut input = original.clone();
        input["project"]["recognitionSettings"]["supportClassFilterMode"] = json!(mode);
        input["project"]["repeatMode"] = json!("single");
        input["project"]["repeatMission"] = json!(false);
        input["project"]["repeatCount"] = json!(99);
        input["project"]["apRecoveryItems"] = json!([]);
        input["project"]["apRecoveryLimits"] = json!({"gold": 10});
        assert_eq!(encode(&input).unwrap(), expected);
    }
    let compact = envelope(&expected);
    assert!(compact["c"]["p"].get("rs").is_none());
    assert!(!compact["f"].as_array().unwrap().contains(&json!(5)));
}

#[test]
fn local_recognition_fields_and_removed_capabilities_are_rejected() {
    let code = encode(&entry()).unwrap();
    for field in ["rs", "cf"] {
        let mut value = envelope(&code);
        value["c"]["p"][field] = if field == "rs" {
            json!({"cf": "allClasses"})
        } else {
            json!("allClasses")
        };
        let error = decode(&packet(&serde_json::to_vec(&value).unwrap())).unwrap_err();
        assert!(error.contains("无法识别的分享协议字段"), "{error}");
    }
    let mut value = envelope(&code);
    value["f"] = json!([5]);
    assert!(decode(&packet(&serde_json::to_vec(&value).unwrap()))
        .unwrap_err()
        .contains("升级"));
}
