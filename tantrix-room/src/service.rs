//! One room's whole server-side lifecycle without I/O, shared by both
//! servers: the driver their adapters feed.
//!
//! A [`RoomService`] owns a room's stored [`RoomRecord`] and the
//! [`RoomEngine`] serving its sockets. Its adapter (the native room actor,
//! or the Durable Object) calls one method per event and keeps only what
//! the service can't: sockets, keepalives and timeouts, the timer or alarm
//! that asks for cleanup, and a [`ServiceHost`] that stores records,
//! claims invitation codes and delivers [`Effect`]s.
//!
//! | Event | Method |
//! | --- | --- |
//! | Room read from storage (or just created) | [`RoomService::new`], then [`load`](RoomService::load) |
//! | Reference request, socket upgrade, sweep or alarm | [`inspect`](RoomService::inspect) |
//! | Socket accepted | [`open`](RoomService::open) |
//! | Frame | [`text`](RoomService::text), [`binary`](RoomService::binary) |
//! | Socket closed | [`close`](RoomService::close) |
//!
//! Each of these but `new` takes the time, in milliseconds.
//!
//! - **Save first.** A changed room is saved, with the archive of a match
//!   it just finished, before the change is committed and before anyone
//!   hears of it ([`RoomEngine::run`]). The record takes the new room only
//!   once storage holds it.
//! - **Presence.** After each event the record notes whether a joined
//!   socket remains ([`RoomRecord::observe_connections`]). It is saved
//!   only if that changed, since a changed room was saved along with it.
//!   Loading always saves the record, so its cleanup is scheduled.
//! - **Cleanup.** [`inspect`](RoomService::inspect) expires the lobby,
//!   releases retired codes and applies retention
//!   ([`lifecycle::cleanup`]), then can allocate a code. A started game
//!   resumes by its permanent link when no code is free.
//! - **Expiry.** An expired lobby closes its sockets with
//!   [`LOBBY_EXPIRED`] and stays a tombstone: `inspect` answers
//!   [`RoomApiError::Expired`], a new socket is closed, frames are ignored.
//! - **Failure.** When storage fails, or retention deletes the room, the
//!   service closes every socket ([`STORAGE_UNAVAILABLE`],
//!   [`ROOM_DELETED`]) and answers every later call with that error
//!   ([`closed`](RoomService::closed)). The adapter drops it, and loads
//!   the room from storage again for the next visitor.

use std::future::Future;

use tantrix_proto::{HostMessage, RoomApiError, RoomReference};

use crate::engine::{ConnId, Connection, Effect, RoomEngine, RoomHost, Step, STORAGE_UNAVAILABLE};
use crate::lifecycle::{self, Catalog, RoomRecord};
use crate::room_logic::{CompletedMatch, RoomData};
use crate::session_cookie::SessionToken;

/// Close reason for the sockets of a lobby that expired.
pub const LOBBY_EXPIRED: &str = "Lobby expired.";
/// Close reason for the sockets of a room that retention deleted.
pub const ROOM_DELETED: &str = "Room deleted.";

/// The I/O a server does for a [`RoomService`]: its [`Catalog`] (room
/// records and invitation codes), match archives, and sockets.
pub trait ServiceHost {
    /// Where room records and invitation codes are stored.
    type Catalog: Catalog;

    /// The host's catalog, for lifecycle cleanup and code allocation.
    fn catalog(&mut self) -> &mut Self::Catalog;

    /// Like [`Catalog::save`], with the archive of the match `record`'s
    /// room just finished, if any, in the same write.
    fn save_record(
        &mut self,
        record: &RoomRecord,
        archive: Option<CompletedMatch>,
    ) -> impl Future<Output = Result<(), RoomApiError>>;

    /// Carry out `effects` in order, and return the sockets that could not
    /// take a message, including any the host no longer has.
    fn deliver(&mut self, effects: Vec<Effect>) -> Vec<ConnId>;

    /// Note what a cleanup did, for logs. Does nothing by default.
    fn cleaned(&mut self, _cleanup: Cleanup) {}
}

/// What [`lifecycle::cleanup`] did to a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cleanup {
    /// An unstarted lobby expired: only a tombstone is left.
    LobbyExpired,
    /// A room kept its state but released its invitation code.
    CodeReleased,
    /// Retention deleted the record: a tombstone, or a room that still had
    /// its state.
    Deleted {
        /// Only a tombstone was left; otherwise the room still had state.
        tombstone: bool,
    },
}

/// A room record and the engine serving its sockets. See the
/// [module docs](self).
#[derive(Debug)]
pub struct RoomService {
    record: RoomRecord,
    /// `None` for a tombstone, and once closed.
    engine: Option<RoomEngine>,
    /// Why every socket was closed; the service is spent.
    closed: Option<RoomApiError>,
}

impl RoomService {
    /// A record as storage holds it, with the sockets still open on it:
    /// none after a process restart; on the Worker, those that outlived the
    /// object's hibernation. Run [`Self::load`] next.
    pub fn new(
        record: RoomRecord,
        connections: impl IntoIterator<Item = (ConnId, Connection)>,
    ) -> Self {
        let engine = record
            .room
            .clone()
            .map(|room| RoomEngine::new(room, connections));
        Self {
            record,
            engine,
            closed: None,
        }
    }

    /// The room record as this service last saved or loaded it.
    pub fn record(&self) -> &RoomRecord {
        &self.record
    }

    /// The room's sockets, unless it is a tombstone or closed.
    pub fn engine(&self) -> Option<&RoomEngine> {
        self.engine.as_ref()
    }

    /// Why the service closed every socket: storage failed (usually
    /// [`RoomApiError::StorageUnavailable`]) or retention deleted the room
    /// ([`RoomApiError::NotFound`]). Drop it then.
    pub fn closed(&self) -> Option<RoomApiError> {
        self.closed
    }

    /// Settle a room just read from storage, or just created: the engine's
    /// [`on_load`](RoomEngine::on_load), who is still connected after it,
    /// then save the record, which schedules its cleanup.
    pub async fn load(
        &mut self,
        host: &mut impl ServiceHost,
        now: u64,
    ) -> Result<(), RoomApiError> {
        self.check()?;
        if let Some(engine) = &self.engine {
            let step = engine.on_load();
            self.run(host, step).await?;
        }
        // Sockets the load found dead are gone by now.
        self.record.observe_connections(self.connected(), now);
        self.save(host).await
    }

    /// Clean the room up, allocate an invitation code if asked, and return
    /// its reference. A tombstone answers [`RoomApiError::Expired`]. An
    /// unstarted lobby answers [`RoomApiError::AllocationBusy`] when no
    /// code is free; a started game is still served without one.
    pub async fn inspect(
        &mut self,
        host: &mut impl ServiceHost,
        allocate: bool,
        now: u64,
    ) -> Result<RoomReference, RoomApiError> {
        self.check()?;
        match self.settle(host, allocate, now).await {
            Err(error @ (RoomApiError::StorageUnavailable | RoomApiError::NotFound)) => {
                Err(self.shut(host, error))
            }
            result => result,
        }
    }

    /// A socket was accepted, with the session token of its handshake. It
    /// hears the room's reference first ([`HostMessage::RoomInfo`]). A
    /// tombstone or closed service closes it.
    pub async fn open(
        &mut self,
        host: &mut impl ServiceHost,
        conn: ConnId,
        token: SessionToken,
        now: u64,
    ) -> Result<(), RoomApiError> {
        if self.engine.is_none() {
            let reason = self.closed.map_or(LOBBY_EXPIRED, close_reason);
            host.deliver(vec![Effect::Close { conn, reason }]);
            return self.check();
        }
        if let Ok(room) = self.record.reference() {
            // A new socket's queue has room; a dead one closes by itself.
            host.deliver(vec![Effect::Send {
                to: conn,
                message: HostMessage::RoomInfo { room },
            }]);
        }
        self.event(host, now, |engine| engine.on_open(conn, token))
            .await
    }

    /// A text frame arrived.
    pub async fn text(
        &mut self,
        host: &mut impl ServiceHost,
        conn: ConnId,
        text: &str,
        now: u64,
    ) -> Result<(), RoomApiError> {
        self.event(host, now, |engine| engine.on_text(conn, text, now))
            .await
    }

    /// A binary frame arrived: the protocol is text only.
    pub async fn binary(
        &mut self,
        host: &mut impl ServiceHost,
        conn: ConnId,
        now: u64,
    ) -> Result<(), RoomApiError> {
        self.event(host, now, |engine| engine.on_binary(conn, now))
            .await
    }

    /// A socket closed (or errored). The host has already let it go.
    pub async fn close(
        &mut self,
        host: &mut impl ServiceHost,
        conn: ConnId,
        now: u64,
    ) -> Result<(), RoomApiError> {
        self.event(host, now, |engine| engine.on_close(conn)).await
    }

    fn check(&self) -> Result<(), RoomApiError> {
        self.closed.map_or(Ok(()), Err)
    }

    fn connected(&self) -> bool {
        self.engine
            .as_ref()
            .is_some_and(|engine| engine.connections().any(|(_, c)| c.joined))
    }

    /// Run the step a socket event makes, then save a new presence. A
    /// tombstone or closed service ignores the event.
    async fn event(
        &mut self,
        host: &mut impl ServiceHost,
        now: u64,
        event: impl FnOnce(&RoomEngine) -> Step,
    ) -> Result<(), RoomApiError> {
        let Some(engine) = &self.engine else {
            return self.check();
        };
        let step = event(engine);
        self.run(host, step).await?;
        if self.record.observe_connections(self.connected(), now) {
            self.save(host).await?;
        }
        Ok(())
    }

    /// Save, commit and deliver `step` ([`RoomEngine::run`]).
    async fn run<H: ServiceHost>(&mut self, host: &mut H, step: Step) -> Result<(), RoomApiError> {
        let Some(engine) = &mut self.engine else {
            return Ok(());
        };
        let mut persist = Persist {
            record: &mut self.record,
            host,
        };
        let result = engine.run(&mut persist, step).await;
        result.map_err(|error| self.shut(host, error))
    }

    async fn save(&mut self, host: &mut impl ServiceHost) -> Result<(), RoomApiError> {
        let result = host.save_record(&self.record, None).await;
        result.map_err(|error| self.shut(host, error))
    }

    async fn settle(
        &mut self,
        host: &mut impl ServiceHost,
        allocate: bool,
        now: u64,
    ) -> Result<RoomReference, RoomApiError> {
        self.record.observe_connections(self.connected(), now);
        self.clean(host, now).await?;
        if self.record.room.is_none() {
            self.close_all(host, LOBBY_EXPIRED);
            return Err(RoomApiError::Expired);
        }
        if allocate {
            // A room coming back for a new code gets a whole inactivity
            // period to use it.
            if self.record.invitation.is_none() && self.record.vacant_since.is_some() {
                self.record.vacant_since = Some(now);
            }
            match lifecycle::allocate(&mut self.record, host.catalog(), lifecycle::new_invitation)
                .await
            {
                Err(RoomApiError::AllocationBusy)
                    if self.record.room.as_ref().is_some_and(|r| r.game.is_some()) => {}
                result => result?,
            }
        }
        self.record.reference()
    }

    /// [`lifecycle::cleanup`], noting what it did.
    async fn clean(&mut self, host: &mut impl ServiceHost, now: u64) -> Result<(), RoomApiError> {
        let had_room = self.record.room.is_some();
        let had_code = self.record.invitation.is_some();
        let result = lifecycle::cleanup(&mut self.record, host.catalog(), now).await;
        let cleanup = match result {
            // Cleanup reports a deleted record as not found.
            Err(RoomApiError::NotFound) => Some(Cleanup::Deleted {
                tombstone: !had_room,
            }),
            Ok(()) if had_room && self.record.room.is_none() => Some(Cleanup::LobbyExpired),
            Ok(()) if had_code && self.record.invitation.is_none() => Some(Cleanup::CodeReleased),
            _ => None,
        };
        if let Some(cleanup) = cleanup {
            host.cleaned(cleanup);
        }
        result
    }

    /// Close every socket for `error` and spend the service. After a failed
    /// save the engine has already closed them all.
    fn shut(&mut self, host: &mut impl ServiceHost, error: RoomApiError) -> RoomApiError {
        self.close_all(host, close_reason(error));
        self.closed = Some(error);
        error
    }

    /// Close every socket with `reason` and drop the engine.
    fn close_all(&mut self, host: &mut impl ServiceHost, reason: &'static str) {
        if let Some(engine) = self.engine.take() {
            host.deliver(
                engine
                    .connections()
                    .map(|(conn, _)| Effect::Close { conn, reason })
                    .collect(),
            );
        }
    }
}

/// Why a closed service closed its sockets.
fn close_reason(error: RoomApiError) -> &'static str {
    match error {
        RoomApiError::NotFound => ROOM_DELETED,
        _ => STORAGE_UNAVAILABLE,
    }
}

/// The engine's [`RoomHost`]: a changed room goes into the record, which
/// takes it only once storage holds it, so a failed save leaves nothing
/// uncommitted for a later save to write.
struct Persist<'a, H> {
    record: &'a mut RoomRecord,
    host: &'a mut H,
}

impl<H: ServiceHost> RoomHost for Persist<'_, H> {
    type Error = RoomApiError;

    async fn save(&mut self, room: &RoomData) -> Result<(), RoomApiError> {
        let mut next = self.record.clone();
        let archive = next.replace_room(room);
        self.host.save_record(&next, archive).await?;
        *self.record = next;
        Ok(())
    }

    fn deliver(&mut self, effects: Vec<Effect>) -> Vec<ConnId> {
        self.host.deliver(effects)
    }
}
