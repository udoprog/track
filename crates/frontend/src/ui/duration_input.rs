use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) value: api::Duration,
    pub(crate) on_change: Callback<api::Duration>,
}

pub(crate) enum Msg {
    AmountChanged(String),
    UnitChanged(String),
}

/// Editor for a duration: a decimal amount plus the unit it is expressed in.
/// Renders as a bare number input and select (no wrapper) so it drops into an
/// existing `input-group`.
///
/// Commits on `change` (blur/Enter) rather than `input`, so a value is emitted
/// once the user finishes editing it. The amount and unit are only re-derived
/// from the incoming value when it changes to something other than what is
/// being edited, so the unit doesn't shift under the user mid-edit.
pub(crate) struct DurationInput {
    amount: String,
    unit: api::DurationUnit,
    /// The duration the local amount and unit currently represent.
    value: api::Duration,
}

impl Component for DurationInput {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let value = ctx.props().value;
        let (amount, unit) = value.split();

        Self {
            amount: amount.to_string(),
            unit,
            value,
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, _: &Props) -> bool {
        let value = ctx.props().value;

        if value == self.value {
            return false;
        }

        let (amount, unit) = value.split();
        self.amount = amount.to_string();
        self.unit = unit;
        self.value = value;
        true
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Msg) -> bool {
        match msg {
            Msg::AmountChanged(amount) => {
                self.amount = amount;
                self.emit(ctx);
                false
            }
            Msg::UnitChanged(unit) => {
                let Ok(unit) = unit.parse() else {
                    return false;
                };

                self.unit = unit;
                self.emit(ctx);
                false
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();

        let on_amount = link.callback(|e: Event| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            Msg::AmountChanged(input.value())
        });

        let on_unit = link.callback(|e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            Msg::UnitChanged(select.value())
        });

        html! {
            <>
                <input type="number" class="input-number fill" min="0" step="any" value={self.amount.clone()} onchange={on_amount} />

                <select class="input-select" onchange={on_unit} value={self.unit.as_str()}>
                    { for api::DurationUnit::ALL.into_iter().map(|unit| html! {
                        <option value={unit.as_str()} selected={unit == self.unit}>{unit.plural()}</option>
                    }) }
                </select>
            </>
        }
    }
}

impl DurationInput {
    /// Emit the edited duration, ignoring an amount that isn't a non-negative
    /// number so in-progress input isn't clobbered.
    fn emit(&mut self, ctx: &Context<Self>) {
        let Ok(amount) = self.amount.trim().parse::<f64>() else {
            return;
        };

        if !amount.is_finite() || amount < 0.0 {
            return;
        }

        let millis = (amount * self.unit.millis() as f64).round() as i64;
        let value = api::Duration::from_millis(millis);
        self.value = value;
        ctx.props().on_change.emit(value);
    }
}
