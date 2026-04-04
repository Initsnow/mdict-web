use std::fs;
use std::path::{Path, PathBuf};

use criterion::{Criterion, criterion_group, criterion_main};
use mdict_web_service::ReloadableDictionaryService;
use tempfile::tempdir;

fn lookup_benchmark(criterion: &mut Criterion) {
    let Some((mdx_path, mdd_paths)) = local_fixture_paths() else {
        return;
    };

    let temp_dir = tempdir().expect("temp dir should be created");
    let config_path = write_config(temp_dir.path(), &mdx_path, &mdd_paths);
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime should start");
    let service = runtime
        .block_on(ReloadableDictionaryService::load_from_path(&config_path))
        .expect("service should load for benchmark");
    let snapshot = runtime.block_on(service.snapshot());

    let mut group = criterion.benchmark_group("lookup");
    group.bench_function("ldoce_apple", |bench| {
        bench.to_async(&runtime).iter(|| async {
            let _ = snapshot
                .lookup("ldoce5pp", "Apple".to_owned())
                .await
                .expect("lookup should succeed");
        });
    });
    group.bench_function("ldoce_suggest_app", |bench| {
        bench.to_async(&runtime).iter(|| async {
            let _ = snapshot
                .suggest("ldoce5pp", "app".to_owned(), Some(10))
                .await
                .expect("suggest should succeed");
        });
    });
    group.bench_function("ldoce_entry_content_apple", |bench| {
        bench.to_async(&runtime).iter(|| async {
            let _ = snapshot
                .entry_content("ldoce5pp", "Apple".to_owned())
                .await
                .expect("entry content should succeed");
        });
    });
    group.finish();
}

fn local_fixture_paths() -> Option<(PathBuf, Vec<PathBuf>)> {
    let candidates = [
        (
            PathBuf::from(
                "/home/initsnow/Documents/Dictionaries/英汉/LDOCE5++/LDOCE5++ V 2-15.mdx",
            ),
            PathBuf::from(
                "/home/initsnow/Documents/Dictionaries/英汉/LDOCE5++/LDOCE5++ V 2-15.mdd",
            ),
        ),
        (
            PathBuf::from("/home/initsnow/projects/mdict-rs/tmp-dict/LDOCE5++/LDOCE5++ V 2-15.mdx"),
            PathBuf::from("/home/initsnow/projects/mdict-rs/tmp-dict/LDOCE5++/LDOCE5++ V 2-15.mdd"),
        ),
    ];

    for (mdx, mdd) in candidates {
        if mdx.exists() && mdd.exists() {
            return Some((mdx, vec![mdd]));
        }
    }

    None
}

fn write_config(dir: &Path, mdx_path: &Path, mdd_paths: &[PathBuf]) -> PathBuf {
    let mdd_paths = mdd_paths
        .iter()
        .map(|path| format!(r#""{}""#, path.display()))
        .collect::<Vec<_>>()
        .join(", ");
    let config = format!(
        r#"
[server]
bind = "127.0.0.1:18080"

[catalog]

[index]
dir = "{}"

[[catalog.bundles]]
dictionary_id = "ldoce5pp"
display_name = "LDOCE5++"
mdx_path = "{}"
mdd_paths = [{}]
"#,
        dir.join("index").display(),
        mdx_path.display(),
        mdd_paths
    );
    let path = dir.join("mdict-web.toml");
    fs::write(&path, config).expect("benchmark config should be written");
    path
}

criterion_group!(benches, lookup_benchmark);
criterion_main!(benches);
