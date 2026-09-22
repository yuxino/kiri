use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};

use iced::futures::{SinkExt, Stream};
use iced::stream::channel;
use iced::widget::{container, row, text};
use iced::{application, Element, Subscription, Task, Theme};

fn main() -> iced::Result {
    application("Iced Example!", update, view)
        .subscription(subscription)
        .theme(|_| Theme::Dark)
        .run_with(new)
}

struct Example {
    last_pressed: String,
    // store the global manager otherwise it will be dropped and events will not be emitted
    _manager: GlobalHotKeyManager,
}

#[derive(Debug, Clone)]
enum ProgramCommands {
    // message received when the subscription calls back to the main gui thread
    Received(String),
}

fn new() -> (Example, Task<ProgramCommands>) {
    let manager = GlobalHotKeyManager::new().unwrap();
    let hotkey_1 = HotKey::new(Some(Modifiers::CONTROL), Code::ArrowRight);
    let hotkey_2 = HotKey::new(None, Code::ArrowUp);

    manager.register(hotkey_1).unwrap();
    manager.register(hotkey_2).unwrap();

    (
        Example {
            last_pressed: "".to_string(),
            _manager: manager,
        },
        Task::none(),
    )
}

fn update(state: &mut Example, msg: ProgramCommands) -> Task<ProgramCommands> {
    match msg {
        ProgramCommands::Received(code) => {
            // update the text widget
            state.last_pressed = code.to_string();

            Task::none()
        }
    }
}

fn view(state: &Example) -> Element<'_, ProgramCommands> {
    container(row![
        text("You pressed: "),
        text(state.last_pressed.clone())
    ])
    .into()
}

fn subscription(_state: &Example) -> Subscription<ProgramCommands> {
    Subscription::run(hotkey_sub)
}

fn hotkey_sub() -> impl Stream<Item = ProgramCommands> {
    channel(32, |mut sender| async move {
        let receiver = GlobalHotKeyEvent::receiver();
        // poll for global hotkey events every 50ms
        loop {
            if let Ok(event) = receiver.try_recv() {
                sender
                    .send(ProgramCommands::Received(format!("{:?}", event)))
                    .await
                    .unwrap();
            }
            async_std::task::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
}
