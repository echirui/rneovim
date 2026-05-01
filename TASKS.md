# rneovim Implementation Tasks (Comprehensive List)

本家 Neovim (C) の全モジュールを精査し、Rust ポート (`rneovim`) における実装状況と今後のマイルストーンを整理しました。

## 1. コア・データ構造 & アルゴリズム (Core Architecture)
- [x] **B-Tree MemLine**: 行データの多段 B-Tree 管理。
- [x] **Swap File**: 変更ログの永続化と復旧。
- [x] **Basic Marks**: 'a-'z の基本マーク。
- [ ] **MarkTree / Extmarks**: バイト単位で座標を追跡する高度なメタデータ基盤。 (本家: `marktree.c`, `extmark.c`)
- [x] **Undo Branching**: アンドゥ履歴の分岐（アンドゥツリー）の完全実装。
- [x] **Persistent Undo**: ファイルを閉じてもアンドゥ履歴を保持。
- [x] **Regex Engine (Vim Flavor)**: Vim 特有の正規表現（`\v`, `\M`, `\zs` 等）への完全対応。

## 2. API 層 (nvim_*) - lazy.nvim 完走マイルストーン
- [x] **Buffer API**:
    - [x] `nvim_buf_set_lines`, `nvim_buf_get_lines`
    - [x] `nvim_buf_set_text`
    - [x] `nvim_buf_get_name`, `nvim_buf_set_name`
    - [x] `nvim_buf_get_option`, `nvim_buf_set_option`
    - [x] `nvim_buf_attach` (バッファ更新の購読 - スタブ)
- [ ] **Window API**:
    - [x] `nvim_open_win` (基本実装)
    - [ ] `nvim_open_win` (フローティングウィンドウの完全な配置ロジック)
    - [x] `nvim_win_set_config`, `nvim_win_get_config`
    - [x] `nvim_win_set_buf`
    - [x] `nvim_win_get_cursor`, `nvim_win_set_cursor`
- [x] **Autocmd API**:
    - [x] `nvim_create_autocmd`, `nvim_exec_autocmds`
    - [x] `once`, `nested` オプションの対応
- [x] **Extmark API**:
    - [x] `nvim_buf_set_extmark` (基本実装)
    - [x] `nvim_buf_get_extmarks`
    - [x] `nvim_create_namespace` (ID管理)
- [x] **Option API**:
    - [x] `nvim_get_option_value`, `nvim_set_option_value`
    - [x] `nvim_get_all_options_info` (デフォルト値含む)

## 3. Lua 統合 & 標準ライブラリ
- [x] **LuaJIT Integration**: `mlua` による実行基盤。
- [ ] **Standard Lua Modules**:
    - [x] `vim.diagnostic`: 診断情報の管理と表示 (スタブ)。
    - [ ] `vim.treesitter`: Tree-sitter 統合の Lua 側レイヤー。
    - [ ] `vim.lsp`: LSP クライアントのコアロジック。
    - [x] `vim.keymap`: `set`, `del` 等の高度なマッピング管理。
    - [ ] `vim.ui`: 共通 UI インターフェース (`select`, `input`)。
- [x] **vim.uv (libuv)**: 主要な fs, timer, check ハンドルの実装。
- [x] **vim.schedule**: メインループとの同期実行。
- [x] **vim.fn (VimL Functions)**: `exists`, `has`, `expand`, `system`, `glob` 等の重要関数の実装。

## 4. UI & 表示エンジン (Engineering Core)
- [ ] **UI Compositor**: 複数バッファ、フローティングウィンドウの重ね合わせ処理。 (`ui_compositor.c`)
- [ ] **Highlight Engine**: ハイライトグループの解決と属性適用。 (`highlight.c`, `highlight_group.c`)
- [ ] **Line Rendering**: 折り返し (wrap)、インデントガイド、行番号表示の詳細制御。 (`drawline.c`)
- [ ] **Screen Update Optimization**: 差分更新アルゴリズムによる描画の高速化。 (`screen.c`)

## 5. プラグイン・エコシステム対応
- [x] **lazy.nvim 起動シーケンス**: `LazyDone`, `LazyVimStarted` までの到達。
- [ ] **lazy.nvim UI 表示**: フローティングウィンドウ内でのコンテンツ描画。
- [ ] **External Process**: `git` 等の外部プロセス実行の安定化 (`system`).

## 6. 実装済み Ex コマンド (Confirmed)
- [x] `:edit`, `:enew`, `:write`, `:saveas`, `:quit`, `:ls`, `:bn`, `:bp`, `:bd`, `:echo`, `:let`, `:messages`, `:pwd`, `:tabnew`, `:tabn`, `:tabp`, `:tabclose`, `:undo`, `:redo`, `:join`, `:ascii`, `:marks`, `:jumps`, `:source`, `:command`, `:augroup`, `:lua`.
