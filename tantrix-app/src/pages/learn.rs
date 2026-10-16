//! A concise, scrolling field guide to the rules this app plays.

use dioxus::prelude::*;
use tantrix_game::{Color, Direction, Move, Position, Rotation, Tile};

use crate::components::learn_board::{DiagramSpace, LearnBoard, SpaceKind};
use crate::components::menu_button::MenuButton;
use crate::components::menu_card::MenuCard;
use crate::components::menu_screen::MenuScreen;
use crate::route::Route;

#[component]
pub fn Learn() -> Element {
    let nav = navigator();

    rsx! {
        MenuScreen { scroll: true,
            div { class: "learn-shell",
                MenuCard {
                    title: "Learn Tantrix",
                    subtitle: "A visual field guide",
                    wide: true,
                    back: move |_| {
                        let _ = nav.push(Route::MainMenu {});
                    },
                    main { class: "learn-guide stack gap-7",
                        p { class: "learn-intro", "2–4 players · 6 tiles each · hands face up" }

                        section {
                            class: "learn-chapter stack gap-4",
                            aria_labelledby: "learn-goal",
                            h2 { id: "learn-goal",
                                span { class: "learn-number", "01" }
                                "Build your longest path."
                            }
                            p { class: "learn-rule",
                                "Choose a color. Connect it into a line or a loop."
                            }
                            div { class: "learn-paths",
                                figure {
                                    LearnBoard {
                                        moves: line_board(),
                                        highlight: Color::Blue,
                                        label: "Three connected tiles form a blue line, scoring three points",
                                    }
                                    figcaption {
                                        strong { "Line: tiles × 1" }
                                        span { "3 tiles = 3 points" }
                                    }
                                }
                                figure {
                                    LearnBoard {
                                        moves: loop_board(),
                                        highlight: Color::Red,
                                        label: "Six connected tiles form a closed red loop, scoring twelve points",
                                    }
                                    figcaption {
                                        strong { "Loop: tiles × 2" }
                                        span { "6 tiles = 12 points" }
                                    }
                                }
                            }
                            p { class: "learn-hint", "Only your best single path counts." }
                        }

                        section {
                            class: "learn-chapter stack gap-4",
                            aria_labelledby: "learn-match",
                            h2 { id: "learn-match",
                                span { class: "learn-number", "02" }
                                "Match every touching edge."
                            }
                            LearnBoard {
                                moves: vec![first_tile(), match_tile()],
                                matched_edge: (Position::CENTER, Direction::Right),
                                label: "The two touching blue edges match, highlighted by a circle",
                            }
                            p { class: "learn-rule",
                                "All colors must match — even your opponents’."
                            }
                        }

                        section {
                            class: "learn-chapter stack gap-4",
                            aria_labelledby: "learn-turn",
                            h2 { id: "learn-turn",
                                span { class: "learn-number", "03" }
                                "A turn can use several tiles."
                            }
                            figure { class: "learn-gap-example",
                                LearnBoard {
                                    moves: gap_board(),
                                    spaces: vec![forced_space()],
                                    label: "Empty space A touches three tiles, so it is a forced space",
                                }
                                figcaption {
                                    strong { "3 neighbors = a forced gap" }
                                    span { "Fill it whenever a tile in your hand fits." }
                                }
                            }
                            ol {
                                class: "learn-turn-flow",
                                aria_label: "The three parts of your turn",
                                li { class: "surface surface-inset",
                                    strong { "Fill gaps" }
                                    span { "if you can" }
                                }
                                li { class: "surface surface-inset",
                                    strong { "Free move" }
                                    span { "one tile" }
                                }
                                li { class: "surface surface-inset",
                                    strong { "Fill again" }
                                    span { "if you can" }
                                }
                            }
                            p { class: "learn-hint",
                                "Draw after each tile while the bag has tiles. Check your new hand too."
                            }
                        }

                        section {
                            class: "learn-chapter stack gap-4",
                            aria_labelledby: "learn-restrictions",
                            h2 { id: "learn-restrictions",
                                span { class: "learn-number", "04" }
                                "Why these spaces are blocked."
                            }
                            p { class: "learn-condition", "While the bag has tiles" }
                            LearnBoard {
                                moves: controlled_board(),
                                spaces: controlled_spaces(),
                                rays: vec![
                                    (
                                        Position::CENTER,
                                        Position::CENTER
                                            .neighbor(Direction::Right)
                                            .neighbor(Direction::Right)
                                            .neighbor(Direction::Right),
                                    ),
                                    (Position::CENTER, Position::CENTER.neighbor(Direction::BottomLeft)),
                                ],
                                label: "Forced gap A controls two straight sides. X, Y, Z and the crossed space are blocked, even when no tile in your hand fits A",
                            }
                            div { class: "learn-key",
                                span {
                                    strong { class: "learn-key-forced", "A" }
                                    " Forced gap"
                                }
                                span {
                                    strong { class: "learn-key-blocked", "X / Y / Z / ×" }
                                    " Blocked sides"
                                }
                            }
                            p { class: "learn-rule",
                                "A blocks play along its straight sides, "
                                strong { "even if you cannot fill A." }
                            }
                            p { class: "learn-hint",
                                "These are controlled sides. Keeping them clear prevents four-sided gaps. Play elsewhere if no forced gap can be filled."
                            }

                            details { class: "learn-restriction",
                                summary { class: "button button-ghost learn-disclosure",
                                    span {
                                        class: "learn-disclosure-arrow",
                                        aria_hidden: "true",
                                        "›"
                                    }
                                    "No fourth neighbor"
                                }
                                figure {
                                    LearnBoard {
                                        moves: gap_board(),
                                        spaces: vec![forced_space()],
                                        preview: side_tile(),
                                        label: "The outlined tile would add a fourth neighbor around the unfilled gap A, which is forbidden while the bag has tiles",
                                    }
                                    figcaption {
                                        strong { "3 + 1 = 4: blocked" }
                                        span { "Fill A before adding another neighbor." }
                                    }
                                }
                            }
                            details { class: "learn-restriction",
                                summary { class: "button button-ghost learn-disclosure",
                                    span {
                                        class: "learn-disclosure-arrow",
                                        aria_hidden: "true",
                                        "›"
                                    }
                                    "No three entries of one color"
                                }
                                figure {
                                    LearnBoard {
                                        moves: same_color_board(),
                                        spaces: vec![
                                            DiagramSpace {
                                                position: Position::CENTER,
                                                label: "×",
                                                kind: SpaceKind::Blocked,
                                            },
                                        ],
                                        highlight: Color::Red,
                                        label: "Three red edges lead into the crossed gap. No tile has three edges of the same color, so this gap cannot be filled",
                                    }
                                    figcaption {
                                        strong { "Red + red + red: blocked" }
                                        span { "No tile has three edges of one color." }
                                    }
                                }
                            }
                        }

                        section {
                            class: "learn-chapter stack gap-4",
                            aria_labelledby: "learn-endgame",
                            h2 { id: "learn-endgame",
                                span { class: "learn-number", "05" }
                                "Empty bag? Restrictions lift."
                            }
                            p { class: "learn-rule",
                                "Controlled sides, fourth neighbors and same-color triples are now allowed."
                            }
                            p { class: "learn-rule",
                                strong { "Still match colors and fill forced gaps whenever possible." }
                            }
                        }
                        footer { class: "learn-footer stack gap-3 items-center",
                            MenuButton {
                                full_width: true,
                                on_click: move |_| {
                                    let _ = nav.push(Route::LocalGame { seed: None });
                                },
                                "Play a game →"
                            }
                            a {
                                class: "button button-ghost",
                                href: "https://www.tantrix.com/english/TantrixGameRules.html",
                                "Full rules ↗"
                            }
                        }
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

fn match_tile() -> Move {
    tile(
        Tile::BYRBRY,
        Position::CENTER.neighbor(Direction::Right),
        Rotation::None,
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
        match_tile(),
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

fn forced_space() -> DiagramSpace {
    DiagramSpace {
        position: Position::CENTER,
        label: "A",
        kind: SpaceKind::Forced,
    }
}

fn controlled_board() -> Vec<Move> {
    let mut board = gap_board();
    board.extend([
        tile(
            Tile::RYYBRB,
            Position::CENTER
                .neighbor(Direction::TopRight)
                .neighbor(Direction::Right),
            Rotation::R60,
        ),
        tile(
            Tile::RYYRBB,
            Position::CENTER
                .neighbor(Direction::TopRight)
                .neighbor(Direction::Right)
                .neighbor(Direction::Right),
            Rotation::R240,
        ),
    ]);
    board
}

fn controlled_spaces() -> Vec<DiagramSpace> {
    vec![
        forced_space(),
        DiagramSpace {
            position: Position::CENTER.neighbor(Direction::Right),
            label: "X",
            kind: SpaceKind::Blocked,
        },
        DiagramSpace {
            position: Position::CENTER
                .neighbor(Direction::Right)
                .neighbor(Direction::Right),
            label: "Y",
            kind: SpaceKind::Blocked,
        },
        DiagramSpace {
            position: Position::CENTER
                .neighbor(Direction::Right)
                .neighbor(Direction::Right)
                .neighbor(Direction::Right),
            label: "Z",
            kind: SpaceKind::Blocked,
        },
        DiagramSpace {
            position: Position::CENTER.neighbor(Direction::BottomLeft),
            label: "×",
            kind: SpaceKind::Blocked,
        },
    ]
}

fn same_color_board() -> Vec<Move> {
    vec![
        tile(
            Tile::RYYBRB,
            Position::CENTER.neighbor(Direction::Left),
            Rotation::R180,
        ),
        tile(
            Tile::BYYBRR,
            Position::CENTER.neighbor(Direction::TopLeft),
            Rotation::R300,
        ),
        tile(
            Tile::YRRBBY,
            Position::CENTER.neighbor(Direction::TopRight),
            Rotation::R240,
        ),
    ]
}

fn side_tile() -> Move {
    tile(
        Tile::YRRBYB,
        Position::CENTER.neighbor(Direction::Right),
        Rotation::None,
    )
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
    use tantrix_game::{Board, Hand, TurnStep};

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
    fn diagrams_match_edges_and_show_the_stated_scores() {
        assert_eq!(board_of(&line_board()).scores()[Color::Blue].score, 3);
        assert_eq!(board_of(&loop_board()).scores()[Color::Red].score, 12);
        board_of(&controlled_board());
        board_of(&same_color_board());
        let gap = board_of(&gap_board());
        assert!(gap.position_is_forced(&Position::CENTER));
        assert!(gap
            .get_possible_move(side_tile().tile, side_tile().position)
            .any(|m| m.rotation == side_tile().rotation));
        let mut fourth = gap_board();
        fourth.push(side_tile());
        let fourth = board_of(&fourth);
        assert_eq!(
            Position::CENTER
                .around()
                .filter(|(p, _)| !fourth.is_available(p))
                .count(),
            4
        );
    }

    #[test]
    fn controlled_diagram_marks_all_and_only_the_controlled_sides() {
        let board = board_of(&controlled_board());
        let blocked = board.controlled_positions(&board.contour_positions());
        let marked = controlled_spaces()
            .into_iter()
            .filter(|space| space.kind == SpaceKind::Blocked)
            .map(|space| space.position)
            .collect();
        assert_eq!(blocked, marked);
        // Y and Z are restricted despite not touching A.
        assert!(!Position::CENTER.around().any(|(p, _)| p
            == Position::CENTER
                .neighbor(Direction::Right)
                .neighbor(Direction::Right)));
        assert!(blocked.contains(
            &Position::CENTER
                .neighbor(Direction::Right)
                .neighbor(Direction::Right)
        ));
        assert!(blocked.contains(
            &Position::CENTER
                .neighbor(Direction::Right)
                .neighbor(Direction::Right)
                .neighbor(Direction::Right)
        ));
    }

    #[test]
    fn same_color_diagram_shows_an_impossible_gap() {
        let moves = same_color_board();
        let board = board_of(&moves);
        let entries: Vec<_> = Position::CENTER
            .around()
            .filter_map(|(p, d)| {
                board
                    .iter()
                    .find(|m| m.position == p)
                    .map(|m| m.color(d.opposite()))
            })
            .collect();
        assert_eq!(entries, vec![Color::Red; 3]);
        let before = board_of(&moves[..2]);
        assert!(before.creates_same_color_triple(&moves[2]));
    }

    #[test]
    fn endgame_lifts_the_side_restriction_but_keeps_forced_priority() {
        let board = board_of(&gap_board());
        let side_only: Hand = serde_json::from_str(r#"{"tiles":["YRRBYB"]}"#).unwrap();
        let with_gap_tile: Hand = serde_json::from_str(r#"{"tiles":["YRRBYB","YGGBBY"]}"#).unwrap();

        assert!(!board
            .possible_moves(&side_only, false, TurnStep::BeforeFreeMove)
            .iter()
            .any(|m| m.placement() == side_tile().placement()));
        assert!(board
            .possible_moves(&side_only, true, TurnStep::BeforeFreeMove)
            .iter()
            .any(|m| m.placement() == side_tile().placement()));
        let forced = board.possible_moves(&with_gap_tile, true, TurnStep::BeforeFreeMove);
        assert!(!forced.is_empty());
        assert!(forced.iter().all(|m| m.forced));
        assert!(!forced
            .iter()
            .any(|m| m.placement() == side_tile().placement()));
    }
}
