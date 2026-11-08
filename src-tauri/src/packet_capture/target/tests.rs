use super::*;

#[test]
fn operations_reject_a_stale_displayed_server_device_or_client() {
    let store = CaptureTargetStore::default();
    let packages = [
        "com.bilibili.fatego",
        "com.bilibili.fgo.mi",
        "com.aniplex.fategrandorder",
    ];
    store
        .select_using(
            Server::Cn,
            None,
            "first",
            packages[1],
            &mut device("first", &packages),
        )
        .unwrap();
    let context: CaptureClientContext = serde_json::from_value(serde_json::json!({
        "serial": "first", "server": "CN", "selectedPackage": packages[1]
    }))
    .unwrap();
    assert!(store
        .resolve_using(Server::Jp, None, Some(&context), &mut |_| panic!(
            "must reject before device I/O"
        ))
        .unwrap_err()
        .contains("服务器已切换"));
    assert!(store
        .resolve_using(
            Server::Cn,
            None,
            Some(&context),
            &mut device("second", &packages)
        )
        .unwrap_err()
        .contains("设备已切换"));
    assert_eq!(
        store
            .resolve_using(
                Server::Cn,
                None,
                Some(&context),
                &mut device("first", &packages)
            )
            .unwrap(),
        ("first".into(), packages[1].into())
    );
    store
        .select_using(
            Server::Cn,
            None,
            "first",
            packages[0],
            &mut device("first", &packages),
        )
        .unwrap();
    assert!(store
        .resolve_using(
            Server::Cn,
            None,
            Some(&context),
            &mut device("first", &packages)
        )
        .unwrap_err()
        .contains("客户端已变更"));
}

fn device<'a>(
    serial: &'a str,
    packages: &'a [&str],
) -> impl FnMut(&[&str]) -> Result<Vec<u8>, String> + 'a {
    move |args| match args {
        ["devices", "-l"] => {
            Ok(format!("List of devices attached\n{serial} device\n").into_bytes())
        }
        ["-s", actual, "shell", "pm list packages --user 0"] if *actual == serial => Ok(packages
            .iter()
            .map(|package| format!("package:{package}\r\n"))
            .collect::<String>()
            .into_bytes()),
        _ => panic!("unexpected device command: {args:?}"),
    }
}

#[test]
fn a_channel_client_works_without_the_official_app_and_multiple_clients_require_a_choice() {
    for channel in [
        "com.bilibili.fgo.mi",
        "com.bilibili.fgo.uc",
        "com.tencent.tmgp.fgo",
        "other.channel.FGO.custom",
    ] {
        let store = CaptureTargetStore::default();
        let packages = [
            "com.android.launcher",
            channel,
            "com.aniplex.fategrandorder",
        ];
        let mut run = device("device", &packages);
        let list = store.list_using(Server::Cn, None, &mut run).unwrap();
        assert_eq!(list.apps.len(), 1);
        assert_eq!(list.selected_package.as_deref(), Some(channel));
        assert!(list.apps[0].name.contains("渠道服"));
        let wire = serde_json::to_value(&list).unwrap();
        assert_eq!(wire["server"], "CN");
        assert_eq!(wire["serial"], "device");
        assert_eq!(wire["selectedPackage"], channel);
        assert_eq!(wire["apps"][0]["packageName"], channel);
        assert_eq!(
            store
                .resolve_using(Server::Cn, None, None, &mut run)
                .unwrap(),
            ("device".into(), channel.into())
        );
    }
    let store = CaptureTargetStore::default();
    let mut run = device("device", &["com.bilibili.fatego", "com.bilibili.fgo.mi"]);
    assert!(store
        .list_using(Server::Cn, None, &mut run)
        .unwrap()
        .selected_package
        .is_none());
    assert_eq!(
        store
            .resolve_using(Server::Cn, None, None, &mut run)
            .unwrap_err(),
        "请在顶部选择需要抓包的客户端，再进行环境配置"
    );
    let chosen = store
        .select_using(Server::Cn, None, "device", "com.bilibili.fgo.mi", &mut run)
        .unwrap();
    assert_eq!(
        chosen.selected_package.as_deref(),
        Some("com.bilibili.fgo.mi")
    );
    assert_eq!(
        store
            .resolve_using(Server::Cn, None, None, &mut run)
            .unwrap()
            .1,
        "com.bilibili.fgo.mi"
    );
}

#[test]
fn selections_are_isolated_by_device_and_server_and_removed_choices_never_fall_back() {
    let store = CaptureTargetStore::default();
    let packages = [
        "com.bilibili.fatego",
        "com.bilibili.fgo.mi",
        "com.aniplex.fategrandorder",
    ];
    let mut run = device("first", &packages);
    store
        .select_using(Server::Cn, None, "first", "com.bilibili.fgo.mi", &mut run)
        .unwrap();
    assert!(store
        .list_using(Server::Cn, None, &mut device("second", &packages))
        .unwrap()
        .selected_package
        .is_none());
    assert_eq!(
        store
            .list_using(Server::Jp, None, &mut run)
            .unwrap()
            .selected_package
            .as_deref(),
        Some("com.aniplex.fategrandorder")
    );
    let mut removed = device("first", &["com.bilibili.fatego"]);
    assert!(store
        .list_using(Server::Cn, None, &mut removed)
        .unwrap()
        .selected_package
        .is_none());
    assert!(store
        .resolve_using(Server::Cn, None, None, &mut removed)
        .is_err());
    store
        .select_using(
            Server::Cn,
            None,
            "first",
            "com.bilibili.fatego",
            &mut removed,
        )
        .unwrap();
    assert_eq!(
        store
            .resolve_using(Server::Cn, None, None, &mut removed)
            .unwrap()
            .1,
        "com.bilibili.fatego"
    );
}

#[test]
fn stale_devices_servers_missing_apps_and_shell_characters_cannot_be_selected() {
    let store = CaptureTargetStore::default();
    let packages = [
        "com.bilibili.fgo.mi",
        "com.aniplex.fategrandorder",
        "com.bilibili.fatego;echo secret",
    ];
    let mut run = device("current", &packages);
    assert!(store
        .select_using(
            Server::Cn,
            None,
            "previous",
            "com.bilibili.fgo.mi",
            &mut run
        )
        .unwrap_err()
        .contains("设备已切换"));
    for package in [
        "com.aniplex.fategrandorder",
        "missing.fgo.app",
        "com.bilibili.fatego;echo secret",
    ] {
        assert!(store
            .select_using(Server::Cn, None, "current", package, &mut run)
            .is_err());
    }
    assert!(store
        .list_using(Server::Cn, Some("offline"), &mut run)
        .is_err());
    let empty = store
        .list_using(Server::Cn, None, &mut device("current", &[]))
        .unwrap();
    assert!(empty.apps.is_empty());
    assert!(empty.selected_package.is_none());
    assert!(store
        .resolve_using(Server::Cn, None, None, &mut device("current", &[]))
        .unwrap_err()
        .contains("请先安装"));
}

#[test]
fn discovery_filters_foreign_servers_and_deduplicates_package_names() {
    let store = CaptureTargetStore::default();
    let mut run = device(
        "current",
        &[
            "com.aniplex.fategrandorder",
            "com.aniplex.fategrandorder.en",
            "com.xiaomeng.fategrandorder",
            "com.netmarble.fgok",
            "com.bilibili.fgo.uc",
            "com.bilibili.fgo.uc",
            "com.reqable.android",
        ],
    );
    let cn = store.list_using(Server::Cn, None, &mut run).unwrap();
    assert_eq!(cn.apps.len(), 1);
    assert_eq!(cn.apps[0].package_name, "com.bilibili.fgo.uc");
    let jp = store.list_using(Server::Jp, None, &mut run).unwrap();
    assert_eq!(jp.apps.len(), 1);
    assert_eq!(jp.apps[0].package_name, "com.aniplex.fategrandorder");
}

#[test]
fn saved_choices_survive_restart_and_remain_scoped_to_device_and_server() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("packet-capture/clients.json");
    let packages = [
        "com.bilibili.fatego",
        "com.bilibili.fgo.mi",
        "com.aniplex.fategrandorder",
    ];
    let store = CaptureTargetStore::new(path.clone());
    store
        .select_using(
            Server::Cn,
            None,
            "first",
            packages[1],
            &mut device("first", &packages),
        )
        .unwrap();
    store
        .select_using(
            Server::Cn,
            None,
            "second",
            packages[0],
            &mut device("second", &packages),
        )
        .unwrap();
    store
        .select_using(
            Server::Jp,
            None,
            "first",
            packages[2],
            &mut device("first", &packages),
        )
        .unwrap();
    drop(store);
    let restored = CaptureTargetStore::new(path);
    for (server, serial, package) in [
        (Server::Cn, "first", packages[1]),
        (Server::Cn, "second", packages[0]),
        (Server::Jp, "first", packages[2]),
    ] {
        let list = restored
            .list_using(server, None, &mut device(serial, &packages))
            .unwrap();
        assert_eq!(list.selected_package.as_deref(), Some(package));
    }
    let removed = restored
        .list_using(Server::Cn, None, &mut device("first", &[packages[0]]))
        .unwrap();
    assert!(removed.selected_package.is_none());
}

#[test]
fn an_automatically_selected_client_is_remembered_after_another_client_is_installed() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("clients.json");
    let store = CaptureTargetStore::new(path.clone());
    store
        .list_using(
            Server::Cn,
            None,
            &mut device("first", &["com.bilibili.fgo.mi"]),
        )
        .unwrap();
    let restored = CaptureTargetStore::new(path);
    let list = restored
        .list_using(
            Server::Cn,
            None,
            &mut device("first", &["com.bilibili.fatego", "com.bilibili.fgo.mi"]),
        )
        .unwrap();
    assert_eq!(
        list.selected_package.as_deref(),
        Some("com.bilibili.fgo.mi")
    );
}

#[test]
fn persistence_failure_keeps_the_current_choice_available_and_reports_a_warning() {
    let directory = tempfile::tempdir().unwrap();
    let blocked = directory.path().join("blocked");
    let store = CaptureTargetStore::new(blocked.join("clients.json"));
    std::fs::write(&blocked, "a file cannot contain preferences").unwrap();
    let packages = ["com.bilibili.fatego", "com.bilibili.fgo.mi"];
    store
        .select_using(
            Server::Cn,
            None,
            "first",
            packages[1],
            &mut device("first", &packages),
        )
        .unwrap();
    assert!(store.take_warning().unwrap().contains("本次运行"));
    let list = store
        .list_using(Server::Cn, None, &mut device("first", &packages))
        .unwrap();
    assert_eq!(list.selected_package.as_deref(), Some(packages[1]));
}

#[test]
fn unreadable_preferences_are_preserved_while_manual_selection_still_works() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("clients.json");
    std::fs::write(&path, "invalid preferences").unwrap();
    let store = CaptureTargetStore::new(path.clone());
    assert!(store.take_warning().is_some());
    let packages = ["com.bilibili.fatego", "com.bilibili.fgo.mi"];
    let list = store
        .select_using(
            Server::Cn,
            None,
            "first",
            packages[1],
            &mut device("first", &packages),
        )
        .unwrap();
    assert_eq!(list.selected_package.as_deref(), Some(packages[1]));
    assert!(store.take_warning().is_some());
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "invalid preferences"
    );
}
