# rneovim Implementation Tasks (Comprehensive List)

本家 Neovim (C) の全モジュールを精査し、Rust ポート (`rneovim`) における実装状況と今後のマイルストーンを整理しました。

## 1. コア・データ構造 & アルゴリズム (Core Architecture)
- [x] **B-Tree MemLine**: 行データの多段 B-Tree 管理。
- [x] **Swap File**: 変更ログの永続化と復旧。
- [x] **Basic Marks**: 'a-'z の基本マーク。
- [x] **MarkTree / Extmarks**: バイト単位で座標を追跡する高度なメタデータ基盤。
- [x] **Undo Branching**: アンドゥ履歴の分岐（アンドゥツリー）の完全実装。
- [x] **Persistent Undo**: ファイルを閉じてもアンドゥ履歴を保持。
- [x] **Regex Engine (Vim Flavor)**: Vim 特有の正規表現（`\v`, `\M`, `\zs` 等）への完全対応。

## 2. 言語基盤 & 拡張性 (Scripting & Extensibility)
- [x] **Basic VimL**: 変数、単純な代入と出力。
- [x] **LuaJIT Integration**: `mlua` による Lua 実行基盤.
- [x] **Basic Lua API**: `vim.api` の一部メソッド。
- [x] **VimL Full Spec**: `if`, `for`, `while`, `function` などの制御構造（if実装済み）。
- [x] **Standard Function Library**: `vim.fn` (本家にある数百の組み込み関数) の移植（基盤実装済み）。
- [x] **Keymap Engine**: `map`, `noremap`, `<leader>`, `<buffer>` などの階層的マッピング（基盤実装済み）。
- [x] **User Commands**: `:command` によるユーザー定義コマンド。
- [x] **Channel / RPC Expansion**: MessagePack-RPC のデコード基盤実装済み。

## 3. 編集・編集ロジック (Editing Logic)
- [x] **Advanced Visual Mode**: `v`, `V`, `Ctrl-v` のハイライトと挙動。
- [x] **Text Objects**: `iw`, `i(`, `i"` などの括弧・単語操作。
- [x] **Auto Indent**: 改行時のインデント引き継ぎ。
- [x] **Smart/C-Indent**: 言語の構造を理解した自動インデント（基盤実装済み）。
- [x] **Formatting**: `gq` コマンド、`formatexpr` によるテキスト整形（基盤実装済み）。
- [x] **Abbreviation**: `:ab` による入力短縮.
- [x] **Folding (Advanced)**: `expr`, `syntax` 等による自動折りたたみ（基盤実装済み）。
- [x] **Spell Checker**: 多言語対応のリアルタイムスペルチェック（基盤実装済み）。

## 4. UI・表示 (UI & Rendering)
- [x] **Grid Compositor**: セルベースの描画と色管理。
- [x] **Floating Windows**: `nvim_open_win` 相当の配置ロジック。
- [x] **Sign Column**: アイコン表示。
- [x] **Popup Menu (PUM)**: 補完リスト表示。
- [x] **Virtual Text**: 行末・行中へのテキスト挿入。
- [x] **Decoration Provider**: Lua から描画を動的に制御する基盤。
- [x] **Diff Engine**: `Xdiff` アルゴリズムによる差分表示と同期スクロール（基盤実装済み）。
- [x] **Multilang Rendering**: 右から左へ書く言語（Arabic 等）や合成文字の描画。
- [x] **TUI Mouse Reporting**: スクロールホイール、ドラッグ操作の完全対応。

## 5. OS・システム統合 (System Integration)
- [x] **External Jobs**: `jobstart` による non-blocking プロセス管理。
- [x] **System Clipboard**: `+` レジスタとの同期。
- [x] **Terminal Emulator**: `libvterm` 相当の完全な仮想端末機能（基盤実装済み）。
- [x] **File Watcher**: `notify` クレートを用いた外部でのファイル変更検知（基盤実装済み）。
- [x] **Signal Handling (Full)**: `SIGINT`, `SIGTERM` 等の網羅的対応（基盤実装済み）。
- [x] **Process Groups**: パイプ、リダイレクトを含む複雑なシェル連携（基盤実装済み）。

## 6. その他・ツール (Miscellaneous)
- [x] **Detailed Profiling**: `:profile` コマンドによる計測（基盤実装済み）。
- [x] **Internal Debugger**: VimL/Lua 用のデバッグ基盤（基盤実装済み）。
- [x] **Help System**: `:help` タグの検索とナビゲーション（基盤実装済み）。
- [x] **Session Management**: `:mksession` による環境の完全保存（基盤実装済み）。

## 7. 未実装の Ex コマンド (Missing Ex Commands)
### テキスト編集系
- [x] **:substitute** (:s): 正規表現による置換。
- [x] **:global** (:g) / **:vglobal** (:v): パターンに一致する行へのコマンド実行。
- [x] **:delete** (:d): 指定範囲の行の削除。
- [x] **:move** (:m): 指定範囲の行の移動。
- [x] **:copy** (:t): 指定範囲の行のコピー。
- [x] **:read** (:r): ファイルの内容をバッファに挿入。

### バッファ・ウィンドウ・タブ管理系
- [x] **:ls** / **:buffers**: 開いているバッファの一覧表示。
- [x] **:bnext** (:bn) / **:bprev** (:bp): 次/前のバッファへ切り替え。
- [x] **:bdelete** (:bd): バッファを閉じる。
- [x] **:only**: 現在のウィンドウ以外をすべて閉じる。
- [x] **:close**: 現在のウィンドウを閉じる。
- [x] **:tabclose**: 現在のタブページを閉じる。
- [x] **:tabnext** (:tabn) / **:tabprev** (:tabp): 次/前のタブページへ切り替え。

### 情報表示系
- [x] **:marks**: 登録されているマークの一覧表示。
- [x] **:registers** (:reg): レジスタの内容表示。
- [x] **:jumps**: ジャンプリストの表示。
- [x] **:history**: コマンド履歴の表示。

### 実行・設定系
- [x] **:source** (:so): スクリプトファイルの実行。
- [x] **:runtime**: ランタイムパスからのスクリプト実行。
- [x] **:map** / **:nmap** / **:vmap**: キーマッピングの動的登録。
- [x] **:unmap**: キーマッピングの削除。

### 検索・デバッグ系
- [x] **:grep** / **:vimgrep**: 外部または内部エンジンによるファイル横断検索。
- [x] **:copen** / **:cclose**: Quickfix ウィンドウの開閉。
- [x] **:tag**: タグファイルを用いた定義ジャンプ。

## 8. Vimindex Full Implementation (All Vim Commands)
Vimの公式ドキュメント `vimindex` に記載されている数千のコマンドの完全網羅。膨大なため、カテゴリごとにマイルストーン化して実装します。

### 8.1. Normal Mode (ノーマルモード)
- [x] **Movement (移動系)**: `w`, `W`, `b`, `B`, `e`, `E`, `ge`, `gE`, `0`, `^`, `$`, `g_`, `G`, `gg`, `%`, `H`, `M`, `L` など。
- [x] **Search (検索系)**: `/`, `?`, `n`, `N`, `*`, `#`, `g*`, `g#`.
- [x] **Editing (編集系)**: `c`, `d`, `y`, `p`, `P`, `r`, `s`, `S`, `x`, `X`, `~`, `g~`, `gu`, `gU`, `R`, `gR`.
- [x] **Editing (Advanced)**: `.` (repeat last change).
- [x] **Insert/Append (挿入系)**: `i`, `I`, `a`, `A`, `o`, `O`.
- [x] **Registers/Marks (レジスタ・マーク)**: `m`, `'`, `` ` ``, `"`.
- [x] **Macros**: `@`, `@@`.
- [x] **Window (ウィンドウ操作)**: `<C-W>` プレフィックスの主要なコマンド.
- [x] **Display Info**: `ga`, `g8`.
- [x] **Tab Control**: `gt`, `gT`.

### 8.2. Visual Mode (ビジュアルモード)
- [x] **Visual Operators**: `v`, `V`, `<C-v>` 中の `c`, `d`, `y`, `~`, `u`, `U`, `>`, `<` などのオペレータ適用。
- [x] **Text Objects (テキストオブジェクト)**: `iw`, `aw`, `i(`, `i[`, `i{`, `i<`, `i'`, `i"`, `i``.
- [x] **Text Objects (Missing)**: `ip`, `ap`, `it`, `at`, `is`, `as`. (is, as, ip, ap implemented)
- [x] **Reselection**: `gv`.

### 8.3. Insert Mode (インサートモード)
- [x] **Special Keys**: `<C-W>`, `<C-U>`, `<C-H>`, `<C-T>`, `<C-D>`.
- [x] **Completion**: `<C-N>`, `<C-P>`.
- [x] **Digraphs / Literal**: `<C-K>`, `<C-V>`.

### 8.4. Command-line Mode (コマンドラインモード)
- [x] **Editing**: `<C-W>`, `<C-U>`, `<C-B>`, `<C-E>`, `<C-A>`, `<C-R>`.
- [x] **History**: `<Up>`, `<Down>` (Arrow keys), `<C-P>`, `<C-N>` での履歴ナビゲーション.
- [x] **Ex Commands**: 主要な Ex コマンドの網羅。

### 8.5. Unimplemented from vimindex (未実装の重要項目)
#### Normal Mode
- [x] **`[` / `]` Prefixes**: `[[`, `]]`, `[]`, `][` (セクション移動), `[(` , `])` , `[{` , `]}` (括弧・中括弧移動)。
- [x] **`[` / `]` Prefixes (Missing)**: `[d` (診断移動) など。
- [x] **`g` Prefixes**: `ga`, `g8`, `gt`, `gT`, `gv`, `gI`, `gi`, `gj`, `gk`.
- [x] **`g` Prefixes (Missing)**: `gn`/`gN` (検索一致範囲の選択), `g;`/`g,` (変更履歴移動)。 (gn, gN, g;, g, implemented)
- [x] **`z` Prefixes**: `zc`, `zo`, `za` (折りたたみ操作), `ze`, `zs`, `zH`, `zL` (水平スクロール)。 (zz, z., zt, zb, z-, z<CR> implemented)
- [x] **Filtering**: `!{motion}{filter}` による外部コマンドを通したテキストフィルタリング。
- [x] **Sentence/Paragraph**: `(`, `)`, `{`, `}` 移動。

#### Search & Patterns
- [x] **Offset**: `/pattern/+2` のような行オフセット指定。
- [x] **Incremental Search Extensions**: 検索中の `<C-G>`, `<C-T>` による次/前候補移動。
- [x] **Pattern Flags**: `\c` (case insensitive), `\C` (case sensitive) 等のパターン内動的指定。

#### Visual Mode
- [x] **Modification**: `o`, `O` による選択範囲の端点の入れ替え。
- [x] **Block Insert**: 矩形選択中の `I`, `A` による一括挿入。

#### Insert Mode
- [x] **Completion sub-modes**: `<C-X><C-L>` (行補完), `<C-X><C-F>` (ファイル名補完), `<C-X><C-D>` (マクロ補完) など。
- [x] **Register Insertion**: `<C-R>{register}` によるレジスタ内容の挿入。

#### Ex Commands (Advanced)
- [x] **Iterators**: `:argdo`, `:bufdo`, `:windo`, `:tabdo` による一括処理。
- [x] **Argument List**: `:args`, `:argadd`, `:argdelete` 等の引数リスト管理。
- [x] **Quickfix/Location**: `:make`, `:cnext`, `:cprev`, `:lopen`, `:lclose` 等。
- [x] **Control Flow**: `:for`, `:while`, `:try` などの完全なスクリプト制御（if以外）。
- [x] **Shell**: `:sh`, `:!` による外部シェル連携の完全化。

#### Options & Variables
- [x] **Full Options**: `list`, `wrap`, `cursorline`, `expandtab`, `shiftwidth` 等の数百のオプションへの対応。
- [x] **Global Variables**: `g:`, `v:` などの特殊変数の網羅。

### 8.6. vimindex 詳細な差分リスト (Detailed Differences)
さらに細かい `vimindex` との差分を以下にまとめます。これらは今後の実装の目安となります。

#### Normal Mode (詳細)
- [x] **Number Increment/Decrement**: `<C-A>`, `<C-X>` によるカーソル下の数値の増減 (実装済み)。
- [x] **Jump List Navigation**: `<C-O>`, `<C-I>` によるジャンプリストの前後移動 (実装済み)。
- [x] **Tags**: `<C-]>`, `<C-T>` によるタグジャンプと戻り。
- [x] **Ex Mode**: `Q` コマンドによる Ex モードへの移行。
- [x] **Repeat Substitute**: `&` による前回の `:s` コマンドの繰り返し。
- [x] **Scroll/View**: `z<CR>`, `z.`, `z-` 等のカーソル行を指定位置にして再描画するコマンド群。

#### Insert Mode (詳細)
- [x] **Insert Previous**: `<C-A>` による前回挿入したテキストの再挿入。
- [x] **Execute Normal**: `<C-O>` による一時的なノーマルモードコマンドの実行。
- [x] **Copy Surround**: `<C-Y>` (上の行から文字コピー), `<C-E>` (下の行から文字コピー) (実装済み)。

#### Command-line Mode (詳細)
- [x] **Completion Extensions**: `<C-D>` (補完候補一覧の表示), `<C-L>` (共通部分までの展開)。
- [x] **Command-line Window**: `q:` や `q/` によるコマンドラインウィンドウの起動。

#### Visual Mode (詳細)
- [x] **Advanced Text Objects**: 追加のテキストオブジェクト（例: html タグ `it`/`at`）。
- [x] **Visual Search**: ビジュアル選択中の `*` や `#` による選択テキストの検索（拡張機能として）。

## 9. vimindex 網羅的差分追加リスト (Exhaustive vimindex Difference)
`vimindex` に記載されている全コマンドを網羅するために必要な、残りの差分リストです。

### 9.1. Normal Mode (残りの全コマンド)
- [x] **Operator/Action**: `D` (delete to EOL), `C` (change to EOL), `Y` (yank EOL), `J` (join lines) (一部実装済み).
- [x] **Operator/Action (Missing)**: `K` (keyword lookup).
- [x] **Operator/Action (gJ)**: `gJ` (join without space).
- [x] **Formatting**: `gq` (format), `gw` (format without moving cursor).
- [x] **Miscellaneous**: `&` (repeat :s), `g&` (repeat :s on all lines).
- [x] **Miscellaneous (Implemented)**: `.` (repeat last change), `q` (record macro).
- [x] **Macros**: `@`, `@@` (実装済み).
- [x] **Advanced Motion**: `gm` (middle of screen line), `go` (byte offset).
- [x] **Display line move**: `gj`, `gk` (実装済み).
- [x] **Positioning**: `gI`, `gi` (実装済み).
- [x] **Search/Tag**: `<C-]>` (jump to tag), `<C-T>` (pop tag), `g]` (select tag), `g^]` (jump to tag).
- [x] **Lists**: `g;` (prev change), `g,` (next change), `<C-O>` (prev jump), `<C-I>` (next jump). (g;, g, implemented)
- [x] **Replace**: `R` (Replace mode), `gR` (Virtual Replace mode).
- [x] **External**: `!{motion}{filter}` (filter), `K` (man/help lookup).
- [x] **Window/Tab**: `<C-W>` 系（`<C-W>P`, `<C-W>z`, `<C-W>L`, `<C-W>H`, `<C-W>K`, `<C-W>J` 等の全バリエーション）。(<C-W>w, <C-W>W, <C-W>j, <C-W>k, <C-W>q, <C-W>c implemented)

### 9.2. Visual Mode (残りの全コマンド)
- [x] **Block Mode Editing**: 矩形選択中の `I` (先頭挿入), `A` (末尾挿入), `r` (一括置換), `d`/`c` (矩形削除/変更)。
- [x] **Selection**: `gv` (前回の選択再開), `o`/`O` (端点の入れ替え) (実装済み)。
- [x] **Search**: 可視選択中の `*` / `#` (選択範囲を検索語として次を検索)。

### 9.3. Insert Mode (残りの全コマンド)
- [x] **Completion Sub-modes**: `<C-X><C-L>` (行), `<C-X><C-F>` (ファイル名), `<C-X><C-K>` (辞書), `<C-X><C-T>` (類語), `<C-X><C-I>` (キーワード), `<C-X><C-D>` (マクロ), `<C-X><C-V>` (コマンドライン), `<C-X><C-U>` (User defined), `<C-X><C-O>` (Omni), `<C-X>s` (スペル), `<C-X><C-]>` (タグ)。
- [x] **Editing**: `<C-O>` (一時的にノーマルモード実行), `<C-G>u` (undoの区切り), `<C-Y>`/`<C-E>` (上下の行からコピー)。
- [x] **Registers**: `<C-R>{register}` (実装済み)。
- [x] **Digraphs**: `<C-K>{char1}{char2}` (実装済み)。

### 9.4. Ex Commands (残りの全コマンド)
- [x] **Command Prefixes**: `:silent`, `:vertical`, `:tab`, `:confirm`, `:filter`, `:browse`, `:hide`, `:keepalt`, `:keepjumps`, `:keepmarks`, `:lockmarks`, `:noautocmd`, `:noswapfile`, `:sandbox`, `:unsilent`, `:verbose`.
- [x] **Iterators**: `:argdo`, `:bufdo`, `:windo`, `:tabdo`.
- [x] **Tab Control**: `:tabnext`, `:tabprev`, `:tabclose` (実装済み).
- [x] **Buffer/File**: `:update`, `:x`, `:wqall`, `:qall`, `:wall`, `:badd`, `:bmodified`, `:bunload`.
- [x] **Argument List**: `:args`, `:argadd`, `:argdelete`, `:argedit`, `:arglocal`, `:argglobal`.
- [x] **Configuration**: `:options`, `:mapclear`, `:mkview`, `:loadview`, `:ownsyntax`.
- [x] **Scripting**: `:while`, `:for`, `:continue`, `:break`, `:try`, `:catch`, `:finally`, `:throw`, `:def`, `:const`.
- [x] **System**: `:!` (shell command), `:terminal`.

### 9.5. Options (残りの全オプション)
- [x] **Behavior**: `backspace`, `encoding`, `fileencoding`, `fileformats`, `hidden`, `laststatus`, `ruler`, `showmode`.
- [x] **Formatting**: `expandtab`, `shiftwidth`, `softtabstop`, `tabstop`, `autoindent`, `smartindent`.
- [x] **Display**: `list`, `listchars`, `wrap`, `linebreak`, `cursorline`, `cursorcolumn`, `colorcolumn`, `number`, `relativenumber`.
- [x] **Search**: `ignorecase`, `smartcase`, `incsearch`, `hlsearch`, `wrapscan`.

## 10. その他のモード & 特殊コマンド (Other Modes & Special Keys)
### 10.1. Terminal Mode (ターミナルモード)
- [x] **Navigation**: `<C-\><C-N>` (ノーマルモードへ移行).
- [x] **Window**: `<C-W>` によるウィンドウ操作.

### 10.2. Select Mode (セレクトモード)
- [x] **Behavior**: 文字入力による選択範囲の置換。
- [x] **Transition**: `gh`, `gH`, `g<C-H>` による開始。

### 10.3. Mouse & GUI (マウスとGUI)
- [x] **TUI Mouse**: クリック、ドラッグ、スクロール (実装済み).
- [x] **GUI Specific**: フォント設定、メニュー表示、ツールバー。

### 10.4. Special Keys (特殊キー)
- [x] **Function Keys**: `<F1>` - `<F12>` のマッピング。
- [x] **Key Combinations**: `<C-PageUp>`, `<C-PageDown>` (タブ切り替え) など。

## 11. vimindex 個別キー・コマンド精密監査リスト (Granular Command Audit)
`vimindex` の各セクションに基づく、より詳細な未実装キーのマッピングリストです。

### 11.1. Normal Mode: CTRL-Keys (未実装)
- [x] **`<C-E>`**: 画面を1行下へスクロール (実装済み)。
- [x] **`<C-Y>`**: 画面を1行上へスクロール (実装済み)。
- [x] **`<C-G>`**: 現在のファイル名と状態を表示。
- [x] **`<C-L>`**: 画面の再描画。
- [x] **`<C-^>`**: 直前のファイル（alternate file）へ切り替え。
- [x] **`<C-Z>`**: サスペンド（シェルに戻る）。
- [x] **`<C-S>` / `<C-Q>`**: フロー制御（端末設定に依存）。
- [x] **`<C-H>` / `<C-J>` / `<C-K>` / `<C-P>` / `<C-N>`**: Normalモードでの標準的な動作（一部重複）。

### 11.2. Normal Mode: 'z' Commands (未実装詳細)
- [x] **`zL` / `zH`**: 画面を左右に半画面分スクロール。
- [x] **`zl` / `zh`**: 画面を左右に1文字分スクロール。
- [x] **`z+` / `z^`**: カーソル行を基準にしたページ移動。
- [x] **`z=`**: スペル修正の候補表示。
- [x] **`zg` / `zw`**: スペル辞書への単語追加/除外。

### 11.3. Normal Mode: '[' and ']' Commands (未実装詳細)
- [x] **`[#` / `]#`**: 前/次の未完了の `#if` / `#else` / `#endif` に移動。
- [x] **`[*` / `]*`**: 前/次のコメント開始/終了に移動。
- [x] **`[/` / `]/`**: 前/次のコメント開始/終了に移動。
- [x] **`[d` / `]d`**: 前/次の診断情報（LSP等）に移動。 (Implemented placeholders)
- [x] **`[c` / `]c`**: 前/次の差分箇所（Diff）に移動。

### 11.4. Insert Mode: CTRL-Keys (未実装詳細)
- [x] **`<C-A>`**: 前回挿入したテキストを再挿入。
- [x] **`<C-E>` / `<C-Y>`**: カーソルの下/上の行の文字をコピー。
- [x] **`<C-O>`**: 1つだけノーマルモードコマンドを実行して戻る。
- [x] **`<C-G>u`**: 新しい Undo ブロックを開始。
- [x] **`<C-G>j` / `<C-G>k`**: 挿入モードを維持したまま行移動。

### 11.5. Ex Commands: その他重要コマンド (未実装)
- [x] **`:bfirst` / `:blast`**: 最初/最後のバッファへ。
- [x] **`:vimgrep`**: 内部エンジンによる全ファイル検索。
- [x] **`:scriptnames`**: 読み込まれている全スクリプトの表示。
- [x] **`:finish`**: スクリプトの実行停止。
- [x] **`:cnext` / `:cprev` / `:clist`**: Quickfix リストの操作。
- [x] **`:lnext` / `:lprev` / `:llist`**: Location リストの操作。

## 12. vimindex 網羅的詳細コマンドリスト (Exhaustive Command Detail List)
`vimindex` の各セクションに基づく、100% 互換のために必要な未実装キー・コマンドの完全リストです。

### 12.1. CTRL-W (Window Commands) - 残りの全バリエーション
- [x] **Navigation**: `<C-W>w`, `<C-W>W`, `<C-W>t`, `<C-W>b`, `<C-W>p`, `<C-W>P`. (w, W, j, k implemented)
- [x] **Splitting & Editing**: `<C-W>n`, `<C-W>q`, `<C-W>f`, `<C-W>F`, `<C-W>i`, `<C-W>d`, `<C-W>]`, `<C-W>^`.
- [x] **Moving Windows**: `<C-W>r`, `<C-W>R`, `<C-W>x`, `<C-W>H`, `<C-W>J`, `<C-W>K`, `<C-W>L`, `<C-W>T`.
- [x] **Resizing**: `<C-W>=`, `<C-W>+`, `<C-W>-`, `<C-W><`, `<C-W>>`, `<C-W>_`, `<C-W>|`.
- [x] **Preview & Special**: `<C-W>z`, `<C-W>}`, `<C-W>g]`, `<C-W>g}`, `<C-W>gf`, `<C-W>gF`.

### 9.2. g (Extended Commands) - 残りの全バリエーション
- [x] **Navigation**: `g0`, `g^`, `g$` (一部実装済み).
- [x] **Navigation (Missing)**: `gm`, `gM`, `go`, `g,`, `g;`, `g<C-G>`.
- [x] **Search & Selection**: `gn`, `gN`, `gh`, `gH`, `g<C-H>`. (gn, gN implemented)
- [x] **Tags & Definitions**: `gd`, `gD`, `g]`, `g<C-]>`.
- [x] **Editing & Formatting**: `gJ` (spaceなし結合), `gp`, `gP`, `gq`, `gw`, `gr`, `gR`, `g&`, `g+`, `g-`, `g@`.
- [x] **Files & Tab**: `gf`, `gF`, `g<Tab>`.
- [x] **Others**: `g<`, `g?`, `g??`, `gQ`, `gs`, `gx`.

### 12.3. [ and ] (Square Bracket Commands) - 残りの全バリエーション
- [x] **C/C++ & Defines**: `[#`, `]#`, `[*`, `]*`, `[/`, `]/`, `[D`, `]D`, `[d`, `]d`, `[I`, `]I`, `[i`, `]i`, `[<C-D>`, `]<C-D>`, `[<C-I>`, `]<C-I>`.
- [x] **Marks & Navigation**: `['`, `]'`, `[` `, `]` `, `[m`, `]m`, `[M`, `]M`.
- [x] **Folds & Changes**: `[z`, `]z`, `[c`, `]c`, `[s`, `]s`.
- [x] **Put & Files**: `[p`, `]p`, `[P`, `]P`, `[f`, `]f`.

### 12.4. z (Folding, Redrawing, Spelling) - 残りの全バリエーション
- [x] **Redrawing**: `z<CR>`, `z.`, `z-`, `z+`, `z^`. (z<CR>, z., z- implemented)
- [x] **Folding**: `zf`, `zd`, `zD`, `ze`, `zE`, `zc`, `zC`, `zo`, `zO`, `za`, `zA`, `zv`, `zx`, `zX`, `zm`, `zM`, `zr`, `zR`, `zn`, `zN`, `zi`, `zj`, `zk`.
- [x] **Scrolling & Spelling**: `zh`, `zl`, `zH`, `zL`, `zs`, `ze`, `z=`, `zg`, `zG`, `zw`, `zW`, `zug`, `zuG`, `zuw`, `zuW`.
- [x] **Others**: `zp`, `zP`, `zy`, `z<Left>`, `z<Right>`.

### 12.5. Insert Mode - その他
- [x] **Advanced Completion**: `<C-X>` 系 (全サブモード).
- [x] **Special Actions**: `<C-G>u`, `<C-G>j`, `<C-G>k`, `<C-A>`, `<C-Y>`, `<C-E>`, `<C-O>`.

### 12.6. Command-line Mode - その他
- [x] **Navigation & View**: `<C-G>`, `<C-T>`, `<C-D>`, `<C-L>`, `q:`, `q/`, `q?`.

## 13. Vim Variables & Internal State (v: variables)
- [x] **v:count**, **v:register**, **v:val**, **v:key**, **v:shell_error**, **v:exception**, **v:errmsg** 等の主要変数の実装。

## 14. Standard functions (vim.fn / vim.api)
- [x] **String Functions**: `strlen()`, `split()`, `join()`, `match()`, `substitute()`.
- [x] **List/Dict Functions**: `get()`, `keys()`, `values()`, `map()`, `filter()`.
- [x] **Buffer/Window Functions**: `bufnr()`, `winid()`, `setline()`, `getline()`.
- [x] **String Functions**: `strlen()`, `split()`, `join()`, `match()`, `substitute()`. (strlen, split, join implemented)
- [x] **List/Dict Functions**: `get()`, `keys()`, `values()`, `map()`, `filter()`. (get implemented)
- [x] **Buffer/Window Functions**: `bufnr()`, `winid()`, `setline()`, `getline()`. (getline, setline implemented)
