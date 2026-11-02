//! 牌譜スナップショット → WebGPU インスタンス列の変換（シーン構築コア）
//!
//! 1インスタンス = 10 floats:
//! [cx, cy, w, h, rot_rad, tile_id, glyph_a, glyph_b, hot, kind]
//!   tile_id: 0..33 = 牌フェイス, -1 = 裏面（赤ドラ 34..36 は通常の5に正規化して渡す）
//!   glyph_a/glyph_b: 字牌・萬子用アトラス番号（-1 = なし, glyph_b=-2 = 中央に単独表示）
//!   hot: 0 = 通常, 1 = 最新の捨て牌（パルス対象）, 2 = 赤ドラ, 3 = 赤ドラ かつ 最新の捨て牌
//!        （kind=3 のグリフでは 0 = 通常, 1 = 金（親）, 2 = 淡黄（ツモ番））
//!   kind: 0 = 牌, 1 = 卓フェルト, 2 = 中央パネル, 3 = グリフ（文字）, 4 = リーチ棒
use wasm_bindgen::prelude::*;

const STRIDE: usize = 10;

// アトラスの追加グリフ番号（JS 側 GLYPHS と一致させること）
const G_DIGIT0: i32 = 17; // '0'..'9' が 17..=26
const G_MINUS: i32 = 27;
const G_KYOKU: i32 = 28; // 局（白）
const G_LWIND: i32 = 29; // 東南西北（白）が 29..=32
const G_LKANJI: i32 = 33; // 一..九（白）が 33..=41
const G_NOKORI: i32 = 42; // 残（白）

/// 赤ドラ（34=赤5萬, 35=赤5筒, 36=赤5索）かどうか
fn is_aka(tile: i32) -> bool {
    (34..=36).contains(&tile)
}

/// 赤ドラを通常の5に、それ以外はそのまま返す
fn base_of(tile: i32) -> i32 {
    match tile {
        34 => 4,
        35 => 13,
        36 => 22,
        t => t,
    }
}

fn glyphs(tile: i32) -> (f32, f32) {
    match tile {
        0..=8 => (tile as f32, 9.0),          // 萬子: 数字 + 萬
        27..=33 => ((tile - 27 + 10) as f32, -2.0), // 字牌: 中央に単独
        _ => (-1.0, -1.0),                    // 筒子・索子はシェーダで手続き描画
    }
}

struct Emitter {
    out: Vec<f32>,
}

impl Emitter {
    fn push(&mut self, cx: f32, cy: f32, w: f32, h: f32, rot: f32, tile: i32, hot: f32, kind: f32) {
        // 赤ドラは通常の5のフェイスで描き、hot に +2 してシェーダへ伝える
        let (tile, hot) = if kind == 0.0 && is_aka(tile) {
            (base_of(tile), hot + 2.0)
        } else {
            (tile, hot)
        };
        let (ga, gb) = if kind == 0.0 && tile >= 0 { glyphs(tile) } else { (-1.0, -1.0) };
        self.out.extend_from_slice(&[cx, cy, w, h, rot, tile as f32, ga, gb, hot, kind]);
    }
    fn tile(&mut self, cx: f32, cy: f32, w: f32, h: f32, rot: f32, tile: i32, hot: f32) {
        self.push(cx, cy, w, h, rot, tile, hot, 0.0);
    }
    /// 文字グリフのクワッド（kind=3）。glyph_b=-2 で中央単独表示の扱いにする
    fn glyph(&mut self, cx: f32, cy: f32, size: f32, rot: f32, idx: i32, hot: f32) {
        self.out.extend_from_slice(&[
            cx, cy, size, size, rot, -1.0, idx as f32, -2.0, hot, 3.0,
        ]);
    }
    /// リーチ棒（kind=4）
    fn stick(&mut self, cx: f32, cy: f32, rot: f32) {
        self.out.extend_from_slice(&[
            cx, cy, 88.0, 10.0, rot, -1.0, -1.0, -1.0, 0.0, 4.0,
        ]);
    }
    /// 風バッジ（kind=5、パネルの四隅。hot=2 で番の家を光らせる）
    fn badge(&mut self, cx: f32, cy: f32, rot: f32, wind_idx: i32, hot: f32) {
        self.out.extend_from_slice(&[
            cx, cy, 36.0, 36.0, rot, -1.0, (G_LWIND + wind_idx) as f32, -2.0, hot, 5.0,
        ]);
    }
}

/// 座席ローカル座標 → ワールド座標（y-down, 中心原点, 盤面 ±500）
fn seat_xy(seat: usize, lx: f32, ly: f32) -> (f32, f32) {
    match seat {
        0 => (lx, ly),
        1 => (ly, -lx),
        2 => (-lx, -ly),
        _ => (-ly, lx),
    }
}

fn seat_rot(seat: usize) -> f32 {
    use std::f32::consts::FRAC_PI_2;
    match seat {
        0 => 0.0,
        1 => FRAC_PI_2,
        2 => std::f32::consts::PI,
        _ => -FRAC_PI_2,
    }
}

fn tiles_of(state: &serde_json::Value, key: &str, seat: usize) -> Vec<i32> {
    state
        .get(key)
        .and_then(|v| v.get(seat))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    if let Some(n) = t.as_i64() {
                        Some(n as i32)
                    } else {
                        t.get("tile").and_then(|n| n.as_i64()).map(|n| n as i32)
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 牌譜 state（JSON文字列）からインスタンス列を作る
#[wasm_bindgen]
pub fn build_scene(state_json: &str) -> Vec<f32> {
    let state: serde_json::Value = match serde_json::from_str(state_json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut e = Emitter { out: Vec::new() };

    // 卓フェルトと中央パネル
    e.push(0.0, 0.0, 1000.0, 1000.0, 0.0, -2, 0.0, 1.0);
    e.push(0.0, 0.0, 300.0, 300.0, 0.0, -3, 0.0, 2.0);

    // ドラ表示牌（捨て牌エリア左上の空きスペースに配置。
    // 卓の左上端は左家の手牌列に隠れるため避ける）
    let dora = state
        .get("dora_indicators")
        .or_else(|| state.get("dora"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for (i, d) in dora.iter().take(5).enumerate() {
        if let Some(t) = d.as_i64() {
            e.tile(-292.0 + i as f32 * 37.0, -218.0, 34.0, 48.0, 0.0, t as i32, 0.0);
        }
    }

    let turn = state.get("turn").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let last_mover = (turn + 3) % 4;
    let dealer = state.get("dealer").and_then(|v| v.as_i64()).unwrap_or(0) as usize % 4;

    // 局表示（パネル中央にグリフで描く：東／南 + 一..四 + 局）
    let round_wind = state.get("round_wind").and_then(|v| v.as_i64()).unwrap_or(0);
    let hand_index = state.get("hand_index").and_then(|v| v.as_i64()).unwrap_or(0);
    e.glyph(-50.0, -26.0, 46.0, 0.0, G_LWIND + round_wind as i32, 0.0);
    e.glyph(0.0, -26.0, 46.0, 0.0, G_LKANJI + hand_index as i32, 0.0);
    e.glyph(50.0, -26.0, 46.0, 0.0, G_KYOKU, 0.0);
    // 残り牌数（雀魂準拠で局表示の下に「残＋数字」）
    if let Some(wall) = state.get("wall_count").and_then(|v| v.as_i64()) {
        let digits: Vec<char> = wall.to_string().chars().collect();
        let total_w = 24.0 + digits.len() as f32 * 19.0;
        let x0 = -total_w / 2.0;
        e.glyph(x0 + 12.0, 30.0, 24.0, 0.0, G_NOKORI, 0.0);
        for (j, ch) in digits.iter().enumerate() {
            if let Some(dig) = ch.to_digit(10) {
                e.glyph(x0 + 24.0 + 9.5 + j as f32 * 19.0, 30.0, 22.0, 0.0,
                        G_DIGIT0 + dig as i32, 0.0);
            }
        }
    }

    // 各家の点数ブロック（パネルの自家側の辺に、風＋点数をグリフで描く）
    let scores: Vec<i64> = state
        .get("scores")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(|n| n.as_i64().unwrap_or(0)).collect())
        .unwrap_or_default();
    let riichi: Vec<bool> = state
        .get("riichi")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(|b| b.as_bool().unwrap_or(false)).collect())
        .unwrap_or_default();
    for seat in 0..4 {
        let rot = seat_rot(seat);
        let wind = (seat + 4 - dealer) % 4;
        // 風バッジ（雀魂準拠でパネルの角の外側。親は赤地、番の家は橙枠）
        let (bx, by) = seat_xy(seat, -167.0, 167.0);
        let hot = match (seat == dealer, seat == turn) {
            (true, true) => 3.0,
            (true, false) => 1.0,
            (false, true) => 2.0,
            (false, false) => 0.0,
        };
        e.badge(bx, by, rot, wind as i32, hot);
        // 点数（辺の中央にセンタリング。ツモ番は淡黄）
        if let Some(score) = scores.get(seat) {
            let hot = if seat == turn { 2.0 } else { 0.0 };
            let text = score.to_string();
            let x0 = -(text.chars().count() as f32) * 27.0 / 2.0 + 13.5;
            for (j, ch) in text.chars().enumerate() {
                let g = match ch {
                    '-' => G_MINUS,
                    '0'..='9' => G_DIGIT0 + (ch as i32 - '0' as i32),
                    _ => continue,
                };
                let (gx, gy) = seat_xy(seat, x0 + j as f32 * 27.0, 110.0);
                e.glyph(gx, gy, 27.0, rot, g, hot);
            }
        }
    }

    for seat in 0..4 {
        let rot = seat_rot(seat);
        // 河（6列グリッド、中心に近い側から）
        let river = tiles_of(&state, "discards", seat);
        for (i, t) in river.iter().enumerate() {
            let col = (i % 6) as f32;
            let row = (i / 6) as f32;
            let (lx, ly) = (-73.0 + col * 36.5, 186.0 + row * 50.0);
            let (x, y) = seat_xy(seat, lx, ly);
            let hot = if seat == last_mover && i + 1 == river.len() { 1.0 } else { 0.0 };
            e.tile(x, y, 34.0, 48.0, rot, *t, hot);
        }
        // 手牌（下家＝視点席は表向き・ソート済み、他家は裏面）
        let mut hand = tiles_of(&state, "hands", seat);
        let n = hand.len();
        if n == 0 {
            continue;
        }
        let (tw, th, gap) = (46.0f32, 64.0f32, 3.0f32);
        let total = n as f32 * (tw + gap) - gap;
        if seat == 0 {
            // 赤ドラは通常の5の隣に並べる（DOM 版ビューアと同じキー）
            hand.sort_unstable_by_key(|&t| base_of(t) * 2 + if is_aka(t) { 1 } else { 0 });
            for (i, t) in hand.iter().enumerate() {
                let lx = -total / 2.0 + tw / 2.0 + i as f32 * (tw + gap);
                let (x, y) = seat_xy(seat, lx, 446.0);
                e.tile(x, y, tw, th, rot, *t, 0.0);
            }
        } else {
            for i in 0..n {
                let lx = -total / 2.0 + tw / 2.0 + i as f32 * (tw + gap);
                let (x, y) = seat_xy(seat, lx, 446.0);
                e.tile(x, y, tw, th, rot, -1, 0.0);
            }
        }
    }

    // リーチ棒は河・手牌より後に（手前に）描く。パネル縁と河の境目に置く
    for seat in 0..4 {
        if riichi.get(seat).copied().unwrap_or(false) {
            let (sx, sy) = seat_xy(seat, 0.0, 156.0);
            e.stick(sx, sy, seat_rot(seat));
        }
    }

    let _ = STRIDE;
    e.out
}

/// インスタンスの float ストライド
#[wasm_bindgen]
pub fn scene_stride() -> usize {
    STRIDE
}
