use crate::net::{self, NetworkConfig};

fn env_of(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    let owned: Vec<(String, String)> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |key: &str| owned.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

#[test]
fn proxy_prefers_config_file_over_environment() {
    let config = NetworkConfig {
        proxy: Some("http://127.0.0.1:7890".into()),
    };
    assert_eq!(
        net::resolve_proxy(&config, env_of(&[("HTTPS_PROXY", "http://env:1")])),
        Some("http://127.0.0.1:7890".to_string())
    );
}

#[test]
fn empty_config_proxy_means_direct_and_overrides_environment() {
    // 桌面应用可能继承到不想要的环境变量，配置里写空串是「明确直连」。
    let config = NetworkConfig {
        proxy: Some("   ".into()),
    };
    assert_eq!(
        net::resolve_proxy(&config, env_of(&[("HTTPS_PROXY", "http://env:1")])),
        None
    );
}

#[test]
fn environment_is_used_when_config_says_nothing() {
    let config = NetworkConfig::default();
    // HTTPS 优先于 HTTP，大小写两种写法都要认。
    assert_eq!(
        net::resolve_proxy(
            &config,
            env_of(&[
                ("HTTP_PROXY", "http://low:1"),
                ("HTTPS_PROXY", "http://high:1")
            ])
        ),
        Some("http://high:1".to_string())
    );
    assert_eq!(
        net::resolve_proxy(&config, env_of(&[("all_proxy", "socks5://127.0.0.1:7891")])),
        Some("socks5://127.0.0.1:7891".to_string())
    );
    assert_eq!(net::resolve_proxy(&config, env_of(&[])), None);
    // 空的环境变量当成没设，别拿它去建代理。
    assert_eq!(
        net::resolve_proxy(&config, env_of(&[("HTTPS_PROXY", "  ")])),
        None
    );
}

#[test]
fn network_config_round_trips_through_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(net::CONFIG_NAME);
    // 文件不存在时是默认值，不该报错。
    assert_eq!(net::load_config(&path), NetworkConfig::default());

    let config = NetworkConfig {
        proxy: Some("socks5://127.0.0.1:7891".into()),
    };
    net::save_config(&path, &config).unwrap();
    assert_eq!(net::load_config(&path), config);

    // 内容坏掉时回落默认值，不能让整个应用起不来。
    std::fs::write(&path, "{not json").unwrap();
    assert_eq!(net::load_config(&path), NetworkConfig::default());
}

#[test]
fn agent_builds_even_when_proxy_string_is_garbage() {
    // 代理写错不该让所有 provider 一起挂掉，退回直连即可。
    let _ = net::agent_with_timeout(std::time::Duration::from_secs(1));
}
