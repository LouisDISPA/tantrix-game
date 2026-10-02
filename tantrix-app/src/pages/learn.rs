//! A short, resettable guide to the rules this app plays.

use dioxus::prelude::*;
use tantrix_game::{Board, Color, Direction, Move, Position, Rotation, Tile};

use crate::components::menu_screen::MenuScreen;
use crate::components::tile::hex_svg::HexSvg;
use crate::route::Route;
use crate::svg::geometry::{axial_to_pixel, board_bounding_box};
use crate::svg::paths::hex_polygon_points;

const STEPS: usize = 5;
const HEX: f64 = 38.0;

#[component]
pub fn Learn() -> Element {
    let mut step = use_signal(|| 0usize);
    let nav = navigator();

    rsx! {
        MenuScreen { scroll: true,
            main { class: "learn-shell",
                header { class: "learn-header",
                    button {
                        class: "learn-back",
                        onclick: move |_| { let _ = nav.push(Route::MainMenu {}); },
                        "← Menu"
                    }
                    span { class: "learn-time", "THE 2-MINUTE GUIDE" }
                }
                div { class: "learn-heading",
                    p { class: "learn-eyebrow", "THE COLOR OF STRATEGY" }
                    h1 { "LEARN TANTRIX" }
                    p { "Five quick moves. Then you’re ready." }
                }
                div { class: "learn-progress", aria_label: "Lesson {step() + 1} of {STEPS}",
                    for index in 0..STEPS {
                        span {
                            key: "{index}",
                            class: if index <= step() { "learn-progress-fill" } else { "" },
                        }
                    }
                }
                section { class: "learn-card",
                    match step() {
                        0 => rsx! { OverviewLesson {} },
                        1 => rsx! { MatchLesson {} },
                        2 => rsx! { ForcedLesson {} },
                        3 => rsx! { EndgameLesson {} },
                        _ => rsx! { ScoreLesson {} },
                    }
                }
                nav { class: "learn-nav", aria_label: "Lesson navigation",
                    button {
                        class: "learn-nav-back",
                        disabled: step() == 0,
                        onclick: move |_| step.set(step().saturating_sub(1)),
                        "← Previous"
                    }
                    span { "{step() + 1} / {STEPS}" }
                    if step() + 1 == STEPS {
                        button {
                            class: "learn-nav-next",
                            onclick: move |_| { let _ = nav.push(Route::LocalGame { seed: None }); },
                            "Play a game →"
                        }
                    } else {
                        button {
                            class: "learn-nav-next",
                            onclick: move |_| step.set((step() + 1).min(STEPS - 1)),
                            "Next →"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn OverviewLesson() -> Element {
    let mut color = use_signal(|| None::<Color>);
    rsx! {
        div { class: "learn-copy",
            p { class: "learn-step", "01 / THE IDEA" }
            h2 { "Make a " strong { "path." } }
            p { class: "learn-rule", "2–4 players. Pick a color. Build its longest " strong { "line" } " or " strong { "loop" } "." }
            p { class: "learn-hint", "Tap a color to trace it." }
            div { class: "learn-actions",
                button { class: "learn-choice color-blue", onclick: move |_| color.set(Some(Color::Blue)), "Blue" }
                button { class: "learn-choice color-red", onclick: move |_| color.set(Some(Color::Red)), "Red" }
                button { class: "learn-reset", onclick: move |_| color.set(None), "Reset ↺" }
            }
            if color().is_some() {
                p { class: "learn-feedback", "One connected path lights up." }
            }
        }
        MiniBoard { moves: line_board(), highlight: color(), label: "Three connected hex tiles" }
    }
}

#[component]
fn MatchLesson() -> Element {
    let mut rotation = use_signal(|| Rotation::R60);
    let mut placed = use_signal(|| false);
    let mut tried = use_signal(|| false);
    let candidate = use_memo(move || match_tile(rotation()));
    let fits = use_memo(move || {
        let candidate = candidate();
        match_board()
            .get_possible_move(candidate.tile, candidate.position)
            .any(|m| m.rotation == candidate.rotation)
    });
    rsx! {
        div { class: "learn-copy",
            p { class: "learn-step", "02 / THE GOLDEN RULE" }
            h2 { "Match " strong { "every edge." } }
            p { class: "learn-rule", "Touching colors must match. " strong { "Always." } }
            p { class: "learn-hint", "Rotate the tile. Then place it." }
            div { class: "learn-actions",
                button {
                    class: "learn-choice",
                    disabled: placed(),
                    onclick: move |_| {
                        rotation.set(Rotation::from(rotation() as i8 + 1));
                        tried.set(false);
                    },
                    "Rotate 60° ↻"
                }
                button {
                    class: "learn-choice learn-choice-primary",
                    disabled: placed(),
                    onclick: move |_| {
                        if fits() { placed.set(true); } else { tried.set(true); }
                    },
                    "Place tile"
                }
                button {
                    class: "learn-reset",
                    onclick: move |_| { rotation.set(Rotation::R60); placed.set(false); tried.set(false); },
                    "Reset ↺"
                }
            }
            if placed() {
                p { class: "learn-feedback", "Matched. The path can grow." }
            } else if tried() {
                p { class: "learn-feedback learn-feedback-error", "These touching edges differ." }
            }
        }
        MiniBoard {
            moves: if placed() { vec![first_tile(), candidate()] } else { vec![first_tile()] },
            target: if placed() { None } else { Some(candidate().position) },
            preview: if placed() { None } else { Some(candidate()) },
            preview_valid: fits(),
            label: "Rotate the right tile until its touching edge matches",
        }
    }
}

#[component]
fn ForcedLesson() -> Element {
    let mut phase = use_signal(|| 0u8);
    let mut tried_free = use_signal(|| false);
    rsx! {
        div { class: "learn-copy",
            p { class: "learn-step", "03 / YOUR TURN" }
            h2 { "Fill the " strong { "gap first." } }
            p { class: "learn-rule", "A gap with " strong { "3+ neighbors" } " is forced if your tile fits." }
            p { class: "learn-hint", "Forced → one free move → forced again. Draw after each tile while the bag has tiles." }
            div { class: "learn-actions",
                button {
                    class: "learn-choice learn-choice-primary",
                    disabled: phase() != 0,
                    onclick: move |_| { phase.set(1); tried_free.set(false); },
                    "Fill gap"
                }
                button {
                    class: "learn-choice",
                    disabled: phase() == 2,
                    onclick: move |_| {
                        if phase() == 0 { tried_free.set(true); } else { phase.set(2); }
                    },
                    "Free move"
                }
                button { class: "learn-reset", onclick: move |_| { phase.set(0); tried_free.set(false); }, "Reset ↺" }
            }
            if phase() == 2 {
                p { class: "learn-feedback", "Now fill any new gaps you can." }
            } else if phase() == 1 {
                p { class: "learn-feedback", "Gap filled. Your free move is next." }
            } else if tried_free() {
                p { class: "learn-feedback learn-feedback-error", "Forced gap first." }
            }
        }
        MiniBoard {
            moves: forced_board(phase()),
            target: if phase() == 0 { Some(Position::CENTER) } else if phase() == 1 { Some(side_tile().position) } else { None },
            target_forced: phase() == 0,
            label: "The center has three neighbors, so it must be filled first",
        }
    }
}

#[component]
fn EndgameLesson() -> Element {
    let mut bag_empty = use_signal(|| false);
    let mut placed = use_signal(|| false);
    let mut tried = use_signal(|| false);
    rsx! {
        div { class: "learn-copy",
            p { class: "learn-step", "04 / WHEN THE BAG EMPTIES" }
            h2 { "The board " strong { "opens up." } }
            p { class: "learn-rule", "With tiles in the bag: no same-color triples, fourth sides, or controlled-side plays." }
            p { class: "learn-hint", "Bag empty? " strong { "No blocking rules. No forced gaps." } " Colors still match; play one tile per turn." }
            div { class: "learn-switch", role: "group", aria_label: "Tiles left in bag",
                button {
                    class: if !bag_empty() { "is-active" } else { "" },
                    onclick: move |_| { bag_empty.set(false); placed.set(false); tried.set(false); },
                    "Bag has tiles"
                }
                button {
                    class: if bag_empty() { "is-active" } else { "" },
                    onclick: move |_| { bag_empty.set(true); placed.set(false); tried.set(false); },
                    "Bag empty"
                }
            }
            div { class: "learn-actions",
                button {
                    class: "learn-choice learn-choice-primary",
                    disabled: placed(),
                    onclick: move |_| {
                        if bag_empty() { placed.set(true); } else { tried.set(true); }
                    },
                    "Try side tile"
                }
                button {
                    class: "learn-reset",
                    onclick: move |_| { bag_empty.set(false); placed.set(false); tried.set(false); },
                    "Reset ↺"
                }
            }
            if placed() {
                p { class: "learn-feedback", "Legal now. The edge colors still match." }
            } else if tried() {
                p { class: "learn-feedback learn-feedback-error", "Blocked while the bag has tiles." }
            }
        }
        MiniBoard {
            moves: endgame_board(placed()),
            target: if placed() { None } else { Some(side_tile().position) },
            target_blocked: !bag_empty(),
            preview: if placed() { None } else { Some(side_tile()) },
            preview_valid: bag_empty(),
            label: "A side placement is blocked before the bag empties and allowed after",
        }
    }
}

#[component]
fn ScoreLesson() -> Element {
    let mut loop_mode = use_signal(|| false);
    rsx! {
        div { class: "learn-copy",
            p { class: "learn-step", "05 / THE WINNER" }
            h2 { "Your " strong { "best path" } " wins." }
            p { class: "learn-rule", "Longest " strong { "line" } ": 1 per tile. Longest " strong { "loop" } ": 2 per tile." }
            p { class: "learn-hint", "Only your best single path counts." }
            div { class: "learn-actions",
                button { class: "learn-choice color-blue", onclick: move |_| loop_mode.set(false), "Trace line" }
                button { class: "learn-choice color-red", onclick: move |_| loop_mode.set(true), "Trace loop" }
                button { class: "learn-reset", onclick: move |_| loop_mode.set(false), "Reset ↺" }
            }
            p { class: "learn-score", class: if loop_mode() { "is-loop" },
                if loop_mode() { "6 tiles × 2 = 12" } else { "3 tiles × 1 = 3" }
            }
        }
        MiniBoard {
            moves: if loop_mode() { loop_board() } else { line_board() },
            highlight: Some(if loop_mode() { Color::Red } else { Color::Blue }),
            label: if loop_mode() { "Six tile red loop scores twelve" } else { "Three tile blue line scores three" },
        }
    }
}

#[component]
fn MiniBoard(
    moves: Vec<Move>,
    #[props(default)] target: Option<Position>,
    #[props(default)] target_forced: bool,
    #[props(default)] target_blocked: bool,
    #[props(default)] preview: Option<Move>,
    #[props(default)] preview_valid: bool,
    #[props(default)] highlight: Option<Color>,
    #[props(into)] label: String,
) -> Element {
    let mut positions: Vec<Position> = moves.iter().map(|m| m.position).collect();
    if let Some(position) = target {
        positions.push(position);
    }
    if let Some(m) = preview {
        positions.push(m.position);
    }
    let (left, top, right, bottom) =
        board_bounding_box(&positions, HEX).unwrap_or((-50.0, -50.0, 50.0, 50.0));
    let view_box = format!(
        "{} {} {} {}",
        left - 36.0,
        top - 36.0,
        right - left + 72.0,
        bottom - top + 72.0
    );
    let mut board = Board::default();
    for &m in &moves {
        board
            .insert_move(m)
            .expect("fixed lesson board has distinct positions");
    }
    let path = highlight.map(|color| board.scoring_paths()[color].positions.clone());

    rsx! {
        div { class: "learn-board",
            svg { view_box, role: "img",
                title { "{label}" }
                if let Some(position) = target {
                    {
                        let (x, y) = axial_to_pixel(position, HEX);
                        let points = hex_polygon_points(x, y, HEX);
                        rsx! { polygon {
                            class: "learn-target",
                            class: if target_forced { "is-forced" },
                            class: if target_blocked { "is-blocked" },
                            points,
                        } }
                    }
                }
                for (index, m) in moves.iter().enumerate() {
                    {
                        let (x, y) = axial_to_pixel(m.position, HEX);
                        let on_path = path.as_ref().is_some_and(|positions| positions.contains(&m.position));
                        let marked = highlight.filter(|_| on_path).into_iter().collect::<Vec<_>>();
                        rsx! { g { key: "{index}",
                            HexSvg {
                                tile: m.tile,
                                rotation: m.rotation,
                                cx: x,
                                cy: y,
                                hex_size: HEX,
                                base_class: if highlight.is_some() { "learn-tile-muted".to_string() } else { String::new() },
                                highlight_colors: marked,
                            }
                        } }
                    }
                }
                if let Some(m) = preview {
                    {
                        let (x, y) = axial_to_pixel(m.position, HEX);
                        rsx! { g {
                            class: "learn-preview",
                            class: if preview_valid { "is-valid" } else { "is-invalid" },
                            HexSvg { tile: m.tile, rotation: m.rotation, cx: x, cy: y, hex_size: HEX }
                        } }
                    }
                }
            }
        }
    }
}

fn tile(tile: Tile, position: Position, rotation: Rotation) -> Move {
    Move {
        tile,
        position,
        rotation,
        forced: false,
    }
}

fn first_tile() -> Move {
    tile(Tile::BYYBRR, Position::CENTER, Rotation::None)
}

fn match_board() -> Board {
    let mut board = Board::default();
    board.insert_move(first_tile()).expect("one tile fits");
    board
}

fn match_tile(rotation: Rotation) -> Move {
    tile(
        Tile::BYRBRY,
        Position::CENTER.neighbor(Direction::Right),
        rotation,
    )
}

fn line_board() -> Vec<Move> {
    vec![
        tile(
            Tile::RYYBRB,
            Position::CENTER.neighbor(Direction::Left),
            Rotation::None,
        ),
        first_tile(),
        match_tile(Rotation::None),
    ]
}

fn gap_board() -> Vec<Move> {
    vec![
        tile(
            Tile::BYYBRR,
            Position::CENTER.neighbor(Direction::Left),
            Rotation::None,
        ),
        tile(
            Tile::YRRBBY,
            Position::CENTER.neighbor(Direction::TopLeft),
            Rotation::None,
        ),
        tile(
            Tile::BYRBRY,
            Position::CENTER.neighbor(Direction::TopRight),
            Rotation::None,
        ),
    ]
}

fn gap_tile() -> Move {
    tile(Tile::YGGBBY, Position::CENTER, Rotation::R180)
}

fn side_tile() -> Move {
    tile(
        Tile::YRRBYB,
        Position::CENTER.neighbor(Direction::Right),
        Rotation::None,
    )
}

fn forced_board(phase: u8) -> Vec<Move> {
    let mut board = gap_board();
    if phase >= 1 {
        board.push(gap_tile());
    }
    if phase >= 2 {
        board.push(side_tile());
    }
    board
}

fn endgame_board(placed: bool) -> Vec<Move> {
    let mut board = gap_board();
    if placed {
        board.push(side_tile());
    }
    board
}

fn loop_board() -> Vec<Move> {
    use Direction::{BottomLeft, BottomRight, Left, Right, TopLeft, TopRight};
    [
        (Left, Tile::BYRBRY, Rotation::None),
        (TopLeft, Tile::YBBRYR, Rotation::None),
        (TopRight, Tile::RYYBRB, Rotation::None),
        (Right, Tile::YRBYBR, Rotation::None),
        (BottomRight, Tile::YGRYRG, Rotation::R240),
        (BottomLeft, Tile::GYYRGR, Rotation::R240),
    ]
    .into_iter()
    .map(|(direction, piece, rotation)| tile(piece, Position::CENTER.neighbor(direction), rotation))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board_of(moves: &[Move]) -> Board {
        let mut board = Board::default();
        for &m in moves {
            board.insert_move(m).unwrap();
        }
        for &m in moves {
            for (neighbor, direction) in m.position.around() {
                if let Some(other) = moves.iter().find(|other| other.position == neighbor) {
                    assert_eq!(m.color(direction), other.color(direction.opposite()));
                }
            }
        }
        board
    }

    #[test]
    fn mini_boards_and_actions_follow_the_game_rules() {
        assert_eq!(board_of(&line_board()).scores()[Color::Blue].score, 3);
        assert_eq!(board_of(&loop_board()).scores()[Color::Red].score, 12);
        let first = match_board();
        assert!(first
            .get_possible_move(Tile::BYRBRY, match_tile(Rotation::None).position)
            .any(|m| m.rotation == Rotation::None));
        assert!(!first
            .get_possible_move(Tile::BYRBRY, match_tile(Rotation::R60).position)
            .any(|m| m.rotation == Rotation::R60));

        let gap = board_of(&gap_board());
        assert!(gap.position_is_forced(&Position::CENTER));
        assert!(gap
            .get_possible_move(gap_tile().tile, Position::CENTER)
            .any(|m| m.rotation == gap_tile().rotation));
        assert!(gap
            .controlled_positions(&gap.contour_positions())
            .contains(&side_tile().position));
        assert!(gap
            .get_possible_move(side_tile().tile, side_tile().position)
            .any(|m| m.rotation == side_tile().rotation));
        board_of(&endgame_board(true));
        board_of(&forced_board(2));
    }
}
