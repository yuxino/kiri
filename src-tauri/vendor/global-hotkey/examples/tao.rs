// Copyright 2022-2022 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use tao::event_loop::{ControlFlow, EventLoopBuilder};

fn main() {
    let event_loop = EventLoopBuilder::new().build();

    let hotkeys_manager = GlobalHotKeyManager::new().unwrap();

    let hotkey = HotKey::new(Some(Modifiers::SHIFT), Code::KeyD);
    let hotkey2 = HotKey::new(Some(Modifiers::SHIFT | Modifiers::ALT), Code::KeyD);
    let hotkey3 = HotKey::new(None, Code::KeyF);
    let hotkey4 = {
        #[cfg(target_os = "macos")]
        {
            HotKey::new(
                Some(Modifiers::SHIFT | Modifiers::ALT),
                Code::MediaPlayPause,
            )
        }
        #[cfg(not(target_os = "macos"))]
        {
            HotKey::new(Some(Modifiers::SHIFT | Modifiers::ALT), Code::MediaPlay)
        }
    };

    hotkeys_manager.register(hotkey).unwrap();
    hotkeys_manager.register(hotkey2).unwrap();
    hotkeys_manager.register(hotkey3).unwrap();
    hotkeys_manager.register(hotkey4).unwrap();

    let global_hotkey_channel = GlobalHotKeyEvent::receiver();

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        if let Ok(event) = global_hotkey_channel.try_recv() {
            println!("{event:?}");

            if hotkey2.id() == event.id && event.state == HotKeyState::Released {
                hotkeys_manager.unregister(hotkey2).unwrap();
            }
        }
    })
}
