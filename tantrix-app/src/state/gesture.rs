//! One or two fingers (or the mouse) on the board, as plain data.
//!
//! The viewport feeds each pointer event to a [`Gesture`] and gets back the
//! new gesture and what the camera should do about it. Nothing here knows
//! the screen: positions are client coordinates, and the camera turns a
//! [`Step`] into a new pose (`svg::camera_math`).

/// How far a pointer may move and still be a tap, in pixels. A finger
/// always wobbles, a mouse hardly does; past this the board follows the
/// pointer and the lift is no click.
pub const TOUCH_SLOP: f64 = 10.0;
pub const MOUSE_SLOP: f64 = 5.0;

/// A pointer at a position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pointer {
    pub id: i32,
    pub x: f64,
    pub y: f64,
}

impl Pointer {
    pub fn new(id: i32, x: f64, y: f64) -> Self {
        Self { id, x, y }
    }

    fn distance_to(&self, other: &Pointer) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }

    fn middle(&self, other: &Pointer) -> (f64, f64) {
        ((self.x + other.x) / 2.0, (self.y + other.y) / 2.0)
    }
}

/// What is going on at the board.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Gesture {
    #[default]
    Idle,
    /// One pointer is down, where it last was. It pans the board once it
    /// has left `start` by more than `slop` (`moved`).
    Pan {
        pointer: Pointer,
        start: (f64, f64),
        moved: bool,
        slop: f64,
    },
    /// Two pointers: the board is pinched and dragged between them.
    Pinch { a: Pointer, b: Pointer },
}

/// What a pointer move asks of the camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    None,
    /// Move the board by (`dx`, `dy`) screen pixels.
    Pan {
        dx: f64,
        dy: f64,
    },
    /// Zoom by `factor` about `from`, then move the board by `to − from`.
    Pinch {
        factor: f64,
        from: (f64, f64),
        to: (f64, f64),
    },
}

impl Gesture {
    /// A pointer went down: the first one starts a pan (that may wobble
    /// `slop` pixels and still be a tap), the second turns it into a pinch,
    /// a third is ignored. The same pointer going down again means its lift
    /// was lost: it starts over.
    pub fn down(self, pointer: Pointer, slop: f64) -> Gesture {
        let pan = Gesture::Pan {
            pointer,
            start: (pointer.x, pointer.y),
            moved: false,
            slop,
        };
        match self {
            Gesture::Idle => pan,
            Gesture::Pan { pointer: first, .. } if first.id == pointer.id => pan,
            Gesture::Pan { pointer: first, .. } => Gesture::Pinch {
                a: first,
                b: pointer,
            },
            other => other,
        }
    }

    /// A pointer moved to `to`: the new gesture and what to do about it.
    /// A pan that has just passed the slop catches up with the whole
    /// distance from where the pointer went down, so the board does not
    /// jump behind the finger.
    pub fn moved(self, to: Pointer) -> (Gesture, Step) {
        match self {
            Gesture::Pan {
                pointer,
                start,
                moved,
                slop,
            } if pointer.id == to.id => {
                let from = if moved { (pointer.x, pointer.y) } else { start };
                let moved_now = moved || pointer_distance(start, &to) > slop;
                let step = if moved_now {
                    Step::Pan {
                        dx: to.x - from.0,
                        dy: to.y - from.1,
                    }
                } else {
                    Step::None
                };
                (
                    Gesture::Pan {
                        pointer: to,
                        start,
                        moved: moved_now,
                        slop,
                    },
                    step,
                )
            }
            Gesture::Pinch { a, b } if a.id == to.id || b.id == to.id => {
                let (a2, b2) = if a.id == to.id { (to, b) } else { (a, to) };
                let before = a.distance_to(&b);
                let factor = if before > 0.0 {
                    a2.distance_to(&b2) / before
                } else {
                    1.0
                };
                (
                    Gesture::Pinch { a: a2, b: b2 },
                    Step::Pinch {
                        factor,
                        from: a.middle(&b),
                        to: a2.middle(&b2),
                    },
                )
            }
            other => (other, Step::None),
        }
    }

    /// A pointer lifted or was cancelled. Lifting one of two fingers leaves
    /// the other panning, with no jump.
    pub fn up(self, id: i32) -> Gesture {
        match self {
            Gesture::Pan { pointer, .. } if pointer.id == id => Gesture::Idle,
            Gesture::Pinch { a, b } if a.id == id || b.id == id => {
                let left = if a.id == id { b } else { a };
                Gesture::Pan {
                    pointer: left,
                    start: (left.x, left.y),
                    moved: true,
                    slop: TOUCH_SLOP,
                }
            }
            other => other,
        }
    }

    /// Whether the board is being moved: a pan past the slop, or a pinch.
    /// A pointer lifted at the end of one is not a click on what is under
    /// it.
    pub fn is_drag(&self) -> bool {
        matches!(
            self,
            Gesture::Pan { moved: true, .. } | Gesture::Pinch { .. }
        )
    }
}

fn pointer_distance(from: (f64, f64), to: &Pointer) -> f64 {
    (to.x - from.0).hypot(to.y - from.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finger(id: i32, x: f64, y: f64) -> Pointer {
        Pointer::new(id, x, y)
    }

    /// The total pan of a series of moves of one pointer from (`x`, `y`).
    fn panned(mut gesture: Gesture, moves: &[(f64, f64)]) -> (Gesture, (f64, f64)) {
        let mut total = (0.0, 0.0);
        for &(x, y) in moves {
            let (next, step) = gesture.moved(finger(1, x, y));
            gesture = next;
            if let Step::Pan { dx, dy } = step {
                total = (total.0 + dx, total.1 + dy);
            }
        }
        (gesture, total)
    }

    #[test]
    fn a_tap_is_not_a_drag() {
        let down = Gesture::Idle.down(finger(1, 100.0, 100.0), TOUCH_SLOP);
        assert!(down != Gesture::Idle && !down.is_drag());
        // A wobble within the slop pans nothing.
        let (wobbled, total) = panned(down, &[(103.0, 98.0), (101.0, 104.0)]);
        assert_eq!(total, (0.0, 0.0));
        assert!(!wobbled.is_drag());
        assert_eq!(wobbled.up(1), Gesture::Idle);
    }

    #[test]
    fn a_pan_catches_up_once_past_the_slop_then_follows_the_finger() {
        let down = Gesture::Idle.down(finger(1, 100.0, 100.0), TOUCH_SLOP);
        // The first move past the slop brings the whole distance so far…
        let (gesture, step) = down.moved(finger(1, 120.0, 100.0));
        assert_eq!(step, Step::Pan { dx: 20.0, dy: 0.0 });
        assert!(gesture.is_drag());
        // …then each move brings its own.
        let (_, step) = gesture.moved(finger(1, 125.0, 110.0));
        assert_eq!(step, Step::Pan { dx: 5.0, dy: 10.0 });
        // The pans add up to where the finger went.
        let (_, total) = panned(down, &[(110.0, 100.0), (120.0, 105.0), (90.0, 140.0)]);
        assert_eq!(total, (-10.0, 40.0));
    }

    #[test]
    fn a_finger_may_wobble_further_than_a_mouse_and_still_tap() {
        // 7px: a mouse has dragged, a finger has only wobbled.
        let (_, step) = Gesture::Idle
            .down(finger(1, 0.0, 0.0), MOUSE_SLOP)
            .moved(finger(1, 7.0, 0.0));
        assert_eq!(step, Step::Pan { dx: 7.0, dy: 0.0 });
        let (touched, step) = Gesture::Idle
            .down(finger(1, 0.0, 0.0), TOUCH_SLOP)
            .moved(finger(1, 7.0, 0.0));
        assert_eq!(step, Step::None);
        assert!(!touched.is_drag());
    }

    #[test]
    fn a_pan_ignores_other_pointers_and_idle_moves() {
        let down = Gesture::Idle.down(finger(1, 0.0, 0.0), TOUCH_SLOP);
        assert_eq!(down.moved(finger(2, 50.0, 50.0)), (down, Step::None));
        assert_eq!(
            Gesture::Idle.moved(finger(1, 5.0, 5.0)),
            (Gesture::Idle, Step::None)
        );
        assert_eq!(down.up(2), down);
        assert_eq!(Gesture::Idle.up(1), Gesture::Idle);
        // The same pointer going down again (its lift was lost) starts over.
        let again = down.down(finger(1, 9.0, 9.0), TOUCH_SLOP);
        assert_eq!(again, Gesture::Idle.down(finger(1, 9.0, 9.0), TOUCH_SLOP));
    }

    #[test]
    fn two_fingers_pinch_by_the_ratio_of_their_distances() {
        let one = Gesture::Idle.down(finger(1, 100.0, 200.0), TOUCH_SLOP);
        let two = one.down(finger(2, 200.0, 200.0), TOUCH_SLOP);
        assert!(matches!(two, Gesture::Pinch { .. }) && two.is_drag());

        // The second finger moves out: 100px apart becomes 150px.
        let (pinched, step) = two.moved(finger(2, 250.0, 200.0));
        assert_eq!(
            step,
            Step::Pinch {
                factor: 1.5,
                from: (150.0, 200.0),
                to: (175.0, 200.0),
            }
        );
        // The next move starts from the new positions.
        let (_, step) = pinched.moved(finger(1, 100.0, 200.0));
        assert_eq!(
            step,
            Step::Pinch {
                factor: 1.0,
                from: (175.0, 200.0),
                to: (175.0, 200.0)
            }
        );
        // Both fingers moving together without changing the distance drags.
        let (_, step) = two.moved(finger(1, 110.0, 210.0));
        let Step::Pinch { factor, .. } = step else {
            panic!("{step:?}");
        };
        assert!(factor < 1.0);
    }

    #[test]
    fn a_third_finger_is_ignored() {
        let two = Gesture::Idle
            .down(finger(1, 0.0, 0.0), TOUCH_SLOP)
            .down(finger(2, 100.0, 0.0), TOUCH_SLOP);
        assert_eq!(two.down(finger(3, 50.0, 50.0), TOUCH_SLOP), two);
        assert_eq!(two.moved(finger(3, 60.0, 60.0)), (two, Step::None));
    }

    #[test]
    fn lifting_one_of_two_fingers_leaves_the_other_panning_without_a_jump() {
        let two = Gesture::Idle
            .down(finger(1, 100.0, 100.0), TOUCH_SLOP)
            .down(finger(2, 200.0, 100.0), TOUCH_SLOP);
        let left = two.up(1);
        assert!(left.is_drag());
        // It pans from where that finger stands, not from where it first went down.
        let (_, step) = left.moved(finger(2, 210.0, 100.0));
        assert_eq!(step, Step::Pan { dx: 10.0, dy: 0.0 });
        assert_eq!(two.up(2).up(1), Gesture::Idle);
    }
}
