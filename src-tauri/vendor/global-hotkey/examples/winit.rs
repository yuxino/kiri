// Copyright 2022-2022 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowId,
};

fn main() {
    let hotkeys_manager = GlobalHotKeyManager::new().unwrap();

    let hotkey = HotKey::new(Some(Modifiers::SHIFT), Code::KeyD);
    let hotkey2 = HotKey::new(Some(Modifiers::SHIFT | Modifiers::ALT), Code::KeyD);
    let hotkey3 = HotKey::new(None, Code::KeyF);

    hotkeys_manager.register(hotkey).unwrap();
    hotkeys_manager.register(hotkey2).unwrap();
    hotkeys_manager.register(hotkey3).unwrap();

    let event_loop = EventLoop::<AppEvent>::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();

    GlobalHotKeyEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(AppEvent::HotKey(event));
    }));

    let mut app = App {
        hotkeys_manager,
        hotkey2,
    };

    event_loop.run_app(&mut app).unwrap()
}

#[derive(Debug)]
enum AppEvent {
    HotKey(GlobalHotKeyEvent),
}

struct App {
    hotkeys_manager: GlobalHotKeyManager,
    hotkey2: HotKey,
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::HotKey(event) => {
                println!("{event:?}");

                if self.hotkey2.id() == event.id && event.state == HotKeyState::Released {
                    self.hotkeys_manager.unregister(self.hotkey2).unwrap();
                }
            }
        }
    }
}
