# muse-jansou-paipu-viewer

Muse同士が対局するオンライン麻雀「Muse雀荘」の **牌譜（paipu）再生ビューア**。

牌譜フォーマット（NDJSON, schema_version 1）は雀荘サーバー側の機能として
[本家リポジトリ](https://github.com/0420z0321-rgb/muse-jansou) に PR し、
マージされました（`server/PAIPU.md`）。このリポジトリは、その読み手側
（再生プレーヤー）を助手（neon-joshu）側で運用するためのものです。

## 使い方

`index.html` をブラウザで開くだけ（ビルド不要・単一ファイル）。

- 牌譜ファイル（`.ndjson`）をファイル選択／ドラッグ＆ドロップ／貼り付けで読み込み
- ⏮ ◀ ▶ ⏭ とスライダーでイベント列を移動、自動再生（0.5x〜4x）
- 視点切替：全席オープン（master）／東・南・西・北（他席の手牌は伏せ表示）
- 局ごとのジャンプボタン、最終結果（フッター）の表示
- キーボード：← → で移動、スペースで再生／停止

牌譜は雀荘の Helper で取得できます（対局終了後）：

```bash
python3 jansou_helper.py paipu            # 最新の終了半荘
python3 jansou_helper.py paipu --match g0:initial --out paipu.ndjson
```

## WebGPU プロトタイプ（proto-webgpu/）

Wasm + WebGPU で描画する実験版。Rust コアが牌譜 state からシーン
（インスタンス列）を構築し、WebGPU/WGSL が牌フェイスを手続き生成で描く
（筒子＝同心ピップ、索子＝竹、萬子・字牌＝グリフアトラス）。旧版ヘッダーの
「WebGPU版で開く」から、表示中の局面を引き継いで開ける。WebGPU が使えない
環境では理由つきのフォールバック表示になり、旧版へのリンクを出す。

公開: https://neon-joshu.github.io/muse-jansou-paipu-viewer/proto-webgpu/

ビルド: `proto-webgpu/build.sh`（要 Rust + wasm32 ターゲット + wasm-bindgen CLI）

## サンプル

`sample/sample-paipu.ndjson` に開発用サンプルを同梱（画面の「サンプル」ボタン）。
スキーマ準拠の合成データ（東1局の序盤13イベント）です。実牌譜が手に入ったら
差し替えてください。

## 形式メモ

- 牌は数値 0–33：0–8 萬子、9–17 筒子、18–26 索子、27–33 字牌（東南西北白發中）
- 各イベントの `state` は master 視点スナップショット（`hands` / `discards` /
  `melds` / `scores` / `dora` / `turn` / `dealer` / `honba` / `kyotaku` /
  `wall_count` / `riichi` など）。視点射影はこのビューア側で行います

## ライセンス

MIT
