use crate::test_support::*;
use crate::ReadPool;

/// 池化最危险的一处：换库时若只替换了池中一条连接，其余连接的文件句柄仍指向旧库，
/// 之后的查询会按轮询随机读到新旧两份数据——恢复备份后界面时对时错，极难排查。
/// 这条锁住「整池替换」这个不变量。
#[test]
fn replacing_the_pool_swaps_every_connection() {
    let dir = tempfile::tempdir().unwrap();
    let old_db = dir.path().join("old.sqlite");
    let new_db = dir.path().join("new.sqlite");
    let old_path = old_db.to_string_lossy().to_string();
    let new_path = new_db.to_string_lossy().to_string();

    let seed = |path: &str, tokens: i64| {
        let conn = store::open_db(path).unwrap();
        let mut record = rec(
            "2026-08-01T10:00:00Z",
            Source::Codex,
            "gpt-5.1-codex",
            "official",
            "/proj/a",
            "s1",
            tokens,
        );
        record.total_tokens = tokens;
        store::insert_records(&conn, &[record]).unwrap();
    };
    seed(&old_path, 100);
    seed(&new_path, 999);

    let pool = ReadPool::open(&old_path, 4).unwrap();
    let prices = PriceTable::default();
    let filter = Filter::default();

    // 取的次数要多于池大小，才能轮到每一条连接。
    let read_all = |pool: &ReadPool| {
        (0..12)
            .map(|_| {
                let conn = pool.get().unwrap();
                query::overview(&conn, &filter, &prices)
                    .unwrap()
                    .total_tokens
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(read_all(&pool), vec![100; 12], "换库前每条连接都读旧库");

    pool.replace_all(|| store::open_readonly(&new_path))
        .unwrap();

    // 漏换任何一条，这里就会混进 100。
    assert_eq!(read_all(&pool), vec![999; 12], "换库后每条连接都必须读新库");
}

/// 池至少要有一条连接，否则 `get()` 会对空 slice 取模除零。
#[test]
fn pool_size_zero_still_yields_a_usable_connection() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.sqlite");
    let path = db.to_string_lossy().to_string();
    store::open_db(&path).unwrap();

    let pool = ReadPool::open(&path, 0).unwrap();
    let conn = pool.get().unwrap();
    assert_eq!(
        query::overview(&conn, &Filter::default(), &PriceTable::default())
            .unwrap()
            .total_tokens,
        0
    );
}
