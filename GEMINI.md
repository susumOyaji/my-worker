# wrangler.jsonc に関する知見

- `wrangler.toml` に存在する `type = "rust"` という設定は、`wrangler.jsonc` のトップレベルには存在しない。
- Rustプロジェクトを `wrangler.jsonc` で設定する場合、`main` フィールドにはビルド前の `.rs` ファイルではなく、ビルド後に生成される **JavaScript のエントリーポイント (`.js` ファイル)** を指定する必要がある。
- `build.command` で `wasm-pack` などを実行し、WasmとJSグルーコードを生成させる構成が一般的。
- `wrangler` の動作フローは、(1) `build.command` を実行 → (2) `main` で指定されたJSファイルをエントリーポイントとしてWorkerを起動、となる。
Wrangler v4ではworker = "0.6.6"クレートが必須です。

wrangler.jsoncの変更は、許可を得てから行うこと。