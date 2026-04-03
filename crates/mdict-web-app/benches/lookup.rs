use std::fs;
use std::path::{Path, PathBuf};

use criterion::{Criterion, criterion_group, criterion_main};
use mdict_web_service::ReloadableDictionaryService;
use tempfile::tempdir;

fn lookup_benchmark(criterion: &mut Criterion) {
    let Some((mdx_path, mdd_path)) = local_fixture_paths() else {
        return;
    };

    let temp_dir = tempdir().expect("temp dir should be created");
    let config_path = write_config(temp_dir.path(), &mdx_path, &mdd_path);
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

fn local_fixture_paths() -> Option<(PathBuf, PathBuf)> {
    let mdx =
        PathBuf::from("/home/initsnow/projects/mdict-rs/tmp-dict/LDOCE5++/LDOCE5++ V 2-15.mdx");
    let mdd =
        PathBuf::from("/home/initsnow/projects/mdict-rs/tmp-dict/LDOCE5++/LDOCE5++ V 2-15.mdd");
    if mdx.exists() && mdd.exists() {
        Some((mdx, mdd))
    } else {
        None
    }
}

fn write_config(dir: &Path, mdx_path: &Path, mdd_path: &Path) -> PathBuf {
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
mdd_path = "{}"
"#,
        dir.join("index").display(),
        mdx_path.display(),
        mdd_path.display()
    );
    let path = dir.join("mdict-web.toml");
    fs::write(&path, config).expect("benchmark config should be written");
    path
}

criterion_group!(benches, lookup_benchmark);
criterion_main!(benches);
