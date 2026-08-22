//! 从上游 LiteLLM 原始价目 JSON 生成内置快照 `assets/litellm_prices.json`。
//!
//! 用法（在仓库根目录）：
//! ```bash
//! curl -sSL https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json \
//!   | cargo run --quiet --bin litellm_snapshot --manifest-path src-tauri/Cargo.toml \
//!   > src-tauri/assets/litellm_prices.json
//! ```
//! 也可传入本地文件路径作为第一个参数；`--as-of YYYY-MM-DD` 可覆盖抓取日期。
//! 与设置页的「可选刷新」共用同一套 [`mabiao_lib::litellm::parse_litellm_raw`]，保证内置与刷新口径一致。

use std::io::Read;

use mabiao_lib::litellm;

fn main() {
    let mut input_path: Option<String> = None;
    let mut as_of: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--as-of" => as_of = args.next(),
            other => input_path = Some(other.to_string()),
        }
    }

    let raw = match input_path {
        Some(path) => {
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {path} 失败：{e}"))
        }
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .expect("从 stdin 读取原始价目失败");
            buf
        }
    };

    let as_of = as_of.unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());

    let snapshot = litellm::parse_litellm_raw(&raw, &as_of).expect("解析 LiteLLM 价目失败");
    let json = serde_json::to_string_pretty(&snapshot).expect("序列化快照失败");
    println!("{json}");
    eprintln!(
        "生成快照：as_of={} 条目={}",
        snapshot.as_of,
        snapshot.entries.len()
    );
}
