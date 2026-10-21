use core::cell::RefCell;
use std::rc::Rc;

use musli_web::web03::prelude::*;
use yew::prelude::*;

/// Tells `onchannel` about the app channel: the open channel each time it
/// opens, and a closed one when it is lost with the connection.
pub(super) struct SetupChannel {
    _channel: Rc<RefCell<ws::Channel>>,
}

impl SetupChannel {
    pub(super) fn new(ws: ws::Handle, onchannel: Callback<Result<ws::Channel, ws::Error>>) -> Self {
        let slot = Rc::new(RefCell::new(ws::Channel::default()));

        // NB: Weak, since the callback is owned by the channel.
        let weak = Rc::downgrade(&slot);

        let channel = ws
            .channel()
            .on_change(move |event| match event {
                ws::ChannelEvent::Opened(..) => {
                    if let Some(slot) = weak.upgrade() {
                        onchannel.emit(Ok(slot.borrow().clone()));
                    }
                }
                ws::ChannelEvent::Failed(error) => onchannel.emit(Err(error)),
                _ => onchannel.emit(Ok(ws::Channel::default())),
            })
            .open();

        *slot.borrow_mut() = channel;
        Self { _channel: slot }
    }
}
