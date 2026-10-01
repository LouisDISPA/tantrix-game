//! Test-only game page: a `VirtualDom` whose root builds its `GameCtx` with
//! `use_game_ctx`, as `LocalGame` and `OnlineGameView` do, and renders
//! nothing. Tests drive the turn actions on it, or read hooks that a test
//! hands in to run on the page, without mounting the game screens.

use dioxus::core::NoOpMutations;
use dioxus::prelude::*;
use futures_channel::mpsc::{unbounded, UnboundedReceiver};
use tantrix_game::{Color, EnumMap};
use tantrix_net::SessionCommand;

use crate::hooks::use_game_ctx;
use crate::state::game_driver::{GameDriver, RoomLink};
use crate::state::game_state::{GameCtx, GameState};
use crate::state::online_state::CommandSender;

/// Let a page catch up with a change: run the memo tasks and effects it
/// queued, and re-render the components it marked dirty. Each round drains
/// what the previous one queued; a handful is more than any change needs.
pub fn settle(dom: &mut VirtualDom) {
    for _ in 0..10 {
        dom.render_immediate(&mut NoOpMutations);
    }
}

/// The online room a page plays in: the seat this device holds (`None`
/// while watching), each seat's nickname, and where its commands go.
#[derive(Clone)]
struct Room {
    seat: Option<Color>,
    names: EnumMap<Color, Option<String>>,
    commands: CommandSender,
}

#[derive(Clone)]
struct Props {
    game: GameState,
    room: Option<Room>,
    /// Hooks the test reads, run on the page after its `GameCtx` exists
    /// (they provide what they build as context).
    hooks: fn(),
}

fn page_root(props: Props) -> Element {
    let Props { game, room, hooks } = props;
    let sender = use_signal(|| room.as_ref().map(|room| room.commands.clone()));
    let seat = room.as_ref().and_then(|room| room.seat);
    let names = room.as_ref().map(|room| room.names.clone());
    let my_color = use_memo(move || seat);
    let names = use_memo(move || names.clone().unwrap_or_default());
    let driver = match room {
        Some(_) => GameDriver::Online(RoomLink::new(sender, my_color, names)),
        None => GameDriver::Local,
    };
    use_game_ctx(driver, move || game);
    hooks();
    rsx! {}
}

pub struct GamePage {
    dom: VirtualDom,
}

impl GamePage {
    /// A local game page showing `game`: every seat is played here.
    pub fn local(game: GameState) -> Self {
        Self::new(game, None, || {})
    }

    /// A local game page showing `game` that also runs `hooks`.
    pub fn local_with(game: GameState, hooks: fn()) -> Self {
        Self::new(game, None, hooks)
    }

    /// An online game page showing `game`, played from `seat` (`None`:
    /// watching), with the commands it sends the room.
    pub fn online(
        game: GameState,
        seat: Option<Color>,
        names: EnumMap<Color, Option<String>>,
    ) -> (Self, UnboundedReceiver<SessionCommand>) {
        let (tx, rx) = unbounded();
        let room = Room {
            seat,
            names,
            commands: CommandSender::new(tx),
        };
        (Self::new(game, Some(room), || {}), rx)
    }

    fn new(game: GameState, room: Option<Room>, hooks: fn()) -> Self {
        let mut dom = VirtualDom::new_with_props(page_root, Props { game, room, hooks });
        dom.rebuild_in_place();
        settle(&mut dom);
        Self { dom }
    }

    pub fn ctx(&self) -> GameCtx {
        self.dom.in_scope(ScopeId::APP, consume_context::<GameCtx>)
    }

    /// Make `change` as an event handler would (outside any render), then
    /// let the page catch up.
    pub fn act(&mut self, change: impl FnOnce(GameCtx)) {
        let ctx = self.ctx();
        self.dom.in_scope(ScopeId::APP, || change(ctx));
        settle(&mut self.dom);
    }

    /// Run `f` on the page: read its state (or a context its hooks
    /// provide) without subscribing to anything, or change it without
    /// letting the page catch up — its effects (the AI loop's, say) wait.
    pub fn with<T>(&self, f: impl FnOnce(GameCtx) -> T) -> T {
        let ctx = self.ctx();
        self.dom.in_scope(ScopeId::APP, || f(ctx))
    }

    /// Read the game, without subscribing to it.
    pub fn game<T>(&self, read: impl FnOnce(&GameState) -> T) -> T {
        self.with(|ctx| read(&ctx.game_state.peek()))
    }
}
