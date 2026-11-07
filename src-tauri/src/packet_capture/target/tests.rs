use super::*;

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
            store.resolve_using(Server::Cn, None, &mut run).unwrap(),
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
        store.resolve_using(Server::Cn, None, &mut run).unwrap_err(),
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
        store.resolve_using(Server::Cn, None, &mut run).unwrap().1,
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
    assert!(store.resolve_using(Server::Cn, None, &mut removed).is_err());
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
            .resolve_using(Server::Cn, None, &mut removed)
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
        .resolve_using(Server::Cn, None, &mut device("current", &[]))
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
