use core::cell::RefCell;
use std::rc::{Rc, Weak};

use musli_web::web03::prelude::*;
use yew::prelude::*;

pub(super) struct SetupChannel {
    _inner: Rc<RefCell<SetupChannelInner>>,
}

struct SetupChannelInner {
    onchannel: Callback<Result<ws::Channel, ws::Error>>,
    _channel_req: ws::Request,
    _state_listener: ws::StateListener,
    state: ws::State,
    ws: ws::Handle,
    on_channel: Callback<Result<ws::Channel, ws::Error>>,
    on_state: Callback<ws::State>,
}

impl SetupChannel {
    pub(super) fn new(ws: ws::Handle, onchannel: Callback<Result<ws::Channel, ws::Error>>) -> Self {
        let this = Self {
            _inner: Rc::new_cyclic(|inner: &Weak<RefCell<SetupChannelInner>>| {
                let on_state = Callback::from({
                    let inner = inner.clone();
                    move |state| {
                        if let Some(inner) = inner.upgrade() {
                            inner.borrow_mut().on_state(state);
                        }
                    }
                });
                let on_channel = Callback::from({
                    let inner = inner.clone();
                    move |ch| {
                        if let Some(inner) = inner.upgrade() {
                            inner.borrow_mut().on_channel(ch);
                        }
                    }
                });
                RefCell::new(SetupChannelInner {
                    onchannel,
                    _channel_req: ws::Request::default(),
                    _state_listener: ws::StateListener::default(),
                    state: ws::State::Closed,
                    ws,
                    on_channel,
                    on_state,
                })
            }),
        };
        this._inner.borrow_mut().setup();
        this
    }
}

impl SetupChannelInner {
    fn on_state(&mut self, state: ws::State) {
        self.state = state;
        self.refresh();
    }

    fn on_channel(&mut self, channel: Result<ws::Channel, ws::Error>) {
        self.onchannel.emit(channel);
    }

    fn setup(&mut self) {
        let (state, listener) = self.ws.on_state_change(self.on_state.clone());
        self.state = state;
        self._state_listener = listener;
        self.refresh();
    }

    fn refresh(&mut self) {
        if self.state.is_open() {
            self._channel_req = self.ws.channel().on_open(self.on_channel.clone()).send();
        } else {
            self.onchannel.emit(Ok(ws::Channel::default()));
        }
    }
}
