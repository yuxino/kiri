// Copyright 2022-2022 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

#![allow(clippy::uninlined_format_args)]

//! global_hotkey lets you register Global HotKeys for Desktop Applications.
//!
//! ## Platforms-supported:
//!
//! - Windows
//! - macOS
//! - Linux (X11 Only)
//!
//! ## Platform-specific notes:
//!
//! - On Windows a win32 event loop must be running on the thread. It doesn't need to be the main thread but you have to create the global hotkey manager on the same thread as the event loop.
//! - On macOS, an event loop must be running on the main thread so you also need to create the global hotkey manager on the main thread.
//!
//! # Example
//!
//! ```no_run
//! use global_hotkey::{GlobalHotKeyManager, hotkey::{HotKey, Modifiers, Code}};
//!
//! // initialize the hotkeys manager
//! let manager = GlobalHotKeyManager::new().unwrap();
//!
//! // construct the hotkey
//! let hotkey = HotKey::new(Some(Modifiers::SHIFT), Code::KeyD);
//!
//! // register it
//! manager.register(hotkey);
//! ```
//!
//!
//! # Processing global hotkey events
//!
//! You can also listen for the menu events using [`GlobalHotKeyEvent::receiver`] to get events for the hotkey pressed events.
//! ```no_run
//! use global_hotkey::GlobalHotKeyEvent;
//!
//! if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
//!     println!("{:?}", event);
//! }
//! ```
//!
//! # Platforms-supported:
//!
//! - Windows
//! - macOS
//! - Linux (X11 Only)

use crossbeam_channel::{unbounded, Receiver, Sender};
use once_cell::sync::{Lazy, OnceCell};

mod error;
pub mod hotkey;
mod platform_impl;

pub use self::error::*;
use hotkey::HotKey;

/// Describes the state of the [`HotKey`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum HotKeyState {
    /// The [`HotKey`] is pressed (the key is down).
    Pressed,
    /// The [`HotKey`] is released (the key is up).
    Released,
}

/// Describes a global hotkey event emitted when a [`HotKey`] is pressed or released.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct GlobalHotKeyEvent {
    /// Id of the associated [`HotKey`].
    pub id: u32,
    /// State of the associated [`HotKey`].
    pub state: HotKeyState,
}

/// A reciever that could be used to listen to global hotkey events.
pub type GlobalHotKeyEventReceiver = Receiver<GlobalHotKeyEvent>;
type GlobalHotKeyEventHandler = Box<dyn Fn(GlobalHotKeyEvent) + Send + Sync + 'static>;

static GLOBAL_HOTKEY_CHANNEL: Lazy<(Sender<GlobalHotKeyEvent>, GlobalHotKeyEventReceiver)> =
    Lazy::new(unbounded);
static GLOBAL_HOTKEY_EVENT_HANDLER: OnceCell<Option<GlobalHotKeyEventHandler>> = OnceCell::new();

impl GlobalHotKeyEvent {
    /// Returns the id of the associated [`HotKey`].
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Returns the state of the associated [`HotKey`].
    pub fn state(&self) -> HotKeyState {
        self.state
    }

    /// Gets a reference to the event channel's [`GlobalHotKeyEventReceiver`]
    /// which can be used to listen for global hotkey events.
    ///
    /// ## Note
    ///
    /// This will not receive any events if [`GlobalHotKeyEvent::set_event_handler`] has been called with a `Some` value.
    pub fn receiver<'a>() -> &'a GlobalHotKeyEventReceiver {
        &GLOBAL_HOTKEY_CHANNEL.1
    }

    /// Set a handler to be called for new events. Useful for implementing custom event sender.
    ///
    /// ## Note
    ///
    /// Calling this function with a `Some` value,
    /// will not send new events to the channel associated with [`GlobalHotKeyEvent::receiver`]
    pub fn set_event_handler<F: Fn(GlobalHotKeyEvent) + Send + Sync + 'static>(f: Option<F>) {
        if let Some(f) = f {
            let _ = GLOBAL_HOTKEY_EVENT_HANDLER.set(Some(Box::new(f)));
        } else {
            let _ = GLOBAL_HOTKEY_EVENT_HANDLER.set(None);
        }
    }

    pub(crate) fn send(event: GlobalHotKeyEvent) {
        if let Some(handler) = GLOBAL_HOTKEY_EVENT_HANDLER.get_or_init(|| None) {
            handler(event);
        } else {
            let _ = GLOBAL_HOTKEY_CHANNEL.0.send(event);
        }
    }
}

pub struct GlobalHotKeyManager {
    platform_impl: platform_impl::GlobalHotKeyManager,
}

impl GlobalHotKeyManager {
    pub fn new() -> crate::Result<Self> {
        Ok(Self {
            platform_impl: platform_impl::GlobalHotKeyManager::new()?,
        })
    }

    pub fn register(&self, hotkey: HotKey) -> crate::Result<()> {
        self.platform_impl.register(hotkey)
    }

    pub fn unregister(&self, hotkey: HotKey) -> crate::Result<()> {
        self.platform_impl.unregister(hotkey)
    }

    pub fn register_all(&self, hotkeys: &[HotKey]) -> crate::Result<()> {
        self.platform_impl.register_all(hotkeys)?;
        Ok(())
    }

    pub fn unregister_all(&self, hotkeys: &[HotKey]) -> crate::Result<()> {
        self.platform_impl.unregister_all(hotkeys)?;
        Ok(())
    }
}
