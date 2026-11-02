//! 牌譜スナップショット → WebGPU インスタンス列の変換（シーン構築コア）
//!
//! 1インスタンス = 10 floats:
//! [cx, cy, w, h, rot_rad, tile_id, glyph_a, glyph_b, hot, kind]
//!   tile_id: 0..33 = 牌フェイス, -1 = 裏面（赤ドラ 34..36 は通常の5に正規化して渡す）
//!   glyph_a/glyph_b: 字牌・萬子用アトラス番号（-1 = なし, glyph_b=-2 = 中央に単独表示）
//!   hot: 0 = 通常, 1 = 最新の捨て牌（パルス対象）, 2 = 赤ドラ, 3 = 赤ドラ かつ 最新の捨て牌
//!   kind: 0 = 牌, 1 = 卓フェルト, 2 = 中央パネル
use wasm_bindgen::prelude::*;

const STRIDE: usize = 10;

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
    e.push(0.0, 0.0, 330.0, 330.0, 0.0, -3, 0.0, 2.0);

    // ドラ表示牌（中央パネル上）
    let dora = state
        .get("dora_indicators")
        .or_else(|| state.get("dora"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for (i, d) in dora.iter().take(5).enumerate() {
        if let Some(t) = d.as_i64() {
            e.tile(-60.0 + i as f32 * 30.0, 62.0, 27.0, 38.0, 0.0, t as i32, 0.0);
        }
    }

    let turn = state.get("turn").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let last_mover = (turn + 3) % 4;

    for seat in 0..4 {
        let rot = seat_rot(seat);
        // 河（6列グリッド、中心に近い側から）
        let river = tiles_of(&state, "discards", seat);
        for (i, t) in river.iter().enumerate() {
            let col = (i % 6) as f32;
            let row = (i / 6) as f32;
            let (lx, ly) = (-73.0 + col * 36.5, 176.0 + row * 50.0);
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

    let _ = STRIDE;
    e.out
}

/// インスタンスの float ストライド
#[wasm_bindgen]
pub fn scene_stride() -> usize {
    STRIDE
}
