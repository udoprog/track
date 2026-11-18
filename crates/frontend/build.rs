fn main() {
    sockel_build::Config::new()
        .local("style/")
        .prefix("track-")
        // Track's classes are not prefixed yet (416edee6 renames them), so
        // every local class is let through for now.
        .foreign(["*"])
        // The Cargo features less the ones they pull in for markup track never
        // renders (Dialog and Confirm footers, Reorder's rows, badges, media,
        // tooltips), so their partials stay out of the CSS.
        .features([
            "button", "confirm", "field", "head", "icons", "input", "layout", "menu", "notice",
            "overlay", "pager", "reorder", "skeleton",
        ])
        .with_module("style/_vars.scss", "v")
        // The app's small text, controls, phone breakpoint, buttons (filled, no
        // edge, touch height on phones), dialogs (centred) and popovers (rounder,
        // wider padded, a sheet's top edge alone), where sockel's differ.
        .with("text-sm", "v.$font-sm")
        .with("control-height", "v.$input-height")
        .with("control-height-small", "v.$input-height-sm")
        .with("breakpoints", r#"("phone": v.$mobile-limit)"#)
        .with("button-text", "v.$font-sm")
        .with("button-text-small", "v.$font-xs")
        .with("button-padding", "v.$padding-lg")
        .with("button-padding-small", "v.$padding-lg")
        .with("button-content-gap", "v.$text-gap")
        .with("button-icon-offset", "-(v.$text-gap)")
        .with("button-phone-height", "v.$input-height-touch")
        .with("radius-lg", "v.$radius-lg")
        .with("glyph-size", "v.$icon-size")
        .with("glyph-phone-size", "v.$icon-size-touch")
        .with("button-disabled-cursor", "default")
        .with("dialog-width", "600px")
        .with("dialog-width-wide", "v.$modal-width-lg")
        .with("dialog-min-height", "0")
        .with("dialog-height-tall", "90vh")
        .with("dialog-sheet-height", "90vh")
        .with("dialog-centred", "true")
        .with("dialog-max-height", "85vh")
        .with("dialog-padding", "v.$gap")
        .with("dialog-body-padding", "v.$gap")
        .with("dialog-body-gap", "v.$gap")
        .with("dialog-title-size", "v.$font-lg")
        .with("dialog-scroll-gutter", "auto")
        .with("dialog-sheet-edge", "top")
        .with("popover-padding", "v.$gap")
        .with("popover-radius", "v.$radius-lg")
        .with("popover-sheet-edge", "top")
        .with("popover-scroll-gutter", "auto")
        .with("popover-sheet-height", "85vh")
        .with("popover-max-width", "100vw")
        .with("menu-width", "0px")
        .with("menu-radius", "v.$radius-lg")
        .with("menu-padding", "v.$gap-xs")
        .with("menu-text", "v.$font-sm")
        .with("menu-item-padding", "0 v.$padding-lg 0 calc(#{v.$padding-lg} - #{v.$text-gap})")
        .with("menu-scroll-gutter", "auto")
        .with("menu-item-gap", "v.$text-gap")
        .with("menu-item-radius", "v.$radius")
        .with("menu-item-hover-fill", "color-mix(in srgb, v.$text-primary 8%, transparent)")
        .with("menu-item-phone-height", "v.$input-height-touch")
        .with("menu-item-mark-ink", "currentColor")
        .with("confirm-gap", "v.$gap")
        // Pill page buttons in muted ink, the current page filled.
        .with("pager-gap", "2px")
        .with("pager-button-radius", "v.$radius-pill")
        .with("pager-button-ink", "v.$text-muted")
        .with("pager-button-text", "v.$font-sm")
        .with("pager-button-hover-fill", "var(--neutral-bg)")
        .with("pager-button-hover-ink", "v.$text-color")
        .with("pager-current-fill", "var(--accent-selected)")
        .with("pager-current-ink", "v.$text-color-selected")
        .with("pager-step-ink", "v.$text-color")
        .with("pager-phone-height", "v.$input-height-touch")
        // Drag handles as high as a control, a touch target on phones.
        .with("drag-handle-size", "v.$input-height")
        .with("drag-handle-phone-size", "v.$input-height-touch")
        .with("drag-handle-radius", "v.$radius-sm")
        .with("drag-handle-hover-fill", "color-mix(in srgb, v.$text-primary 8%, transparent)")
        // Edged and square, a line as high as a small control, a block a touch
        // control high on phones.
        .with("skeleton-fill", "v.$bg-dark-transparent")
        .with("skeleton-edge", "v.$border-subtle-transparent")
        .with("skeleton-radius", "0")
        .with("skeleton-line-height", "v.$input-height-sm")
        .with("skeleton-line-inset", "0")
        .with("skeleton-phone-height", "v.$input-height-touch")
        // A raised card: the surface, a soft danger edge and a deep shadow, the
        // message in medium weight over its causes, a line each, nothing under
        // them.
        .with("error-box-padding", "v.$gap-sm v.$gap-sm v.$gap-sm v.$gap")
        .with("error-box-gap", "v.$gap-sm")
        .with("error-box-fill", "v.$bg-surface")
        .with("error-box-edge", "color-mix(in srgb, v.$color-danger 45%, transparent)")
        .with("error-box-shadow", "0 8px 32px v.$shadow-strong")
        .with("error-box-message-weight", "v.$bold-weight")
        .with("error-box-cause-font", "inherit")
        .with("error-box-cause-text", "v.$font-sm")
        .with("error-box-text-end", "0")
        // Text fields: the edge strengthens on hover and turns accent while
        // focused, the ring over it; small text, a touch target high on phones.
        .with("input-hover-edge", "var(--field-border-hover)")
        .with("input-focus-edge", "v.$color-primary")
        .with("input-ring-offset", "-1px")
        .with("input-padding-inline", "v.$padding-lg + 2px")
        .with("input-text", "v.$font-sm")
        .with("input-phone-height", "v.$input-height-touch")
        // Settings' label column; hints in the text's own line height.
        .with("field-label-width", "200px")
        .with("field-message-line", "normal")
        // Defined by the theme (_theme.scss).
        .with("danger", "false")
        .with("success", "false")
        .with("warning", "false")
        .with("on-accent", "false")
        .with("scrim", "false")
        .with("field-bg", "false")
        .with("field-border", "false")
        .with("card-border-width", "false")
        // The parts of sockel's enabled features track never renders.
        .with(
            "omit",
            r#"(
            "buttons.bare", "buttons.busy", "buttons.fold", "buttons.touch",
            "confirm.inline",
            "fields.busy", "fields.check", "fields.disabled", "fields.error", "fields.field",
            "fields.fieldset", "fields.help", "fields.required",
            "heads.crumbs", "heads.label", "heads.list", "heads.trail",
            "inputs.busy", "inputs.disabled", "inputs.group", "inputs.invalid", "inputs.mark",
            "inputs.small", "inputs.textarea", "inputs.touch",
            "layout.card-compact", "layout.card-dense", "layout.card-link", "layout.flush",
            "layout.gap-hair", "layout.gap-medium", "layout.gap-none", "layout.gap-wide",
            "layout.grow", "layout.lines", "layout.pages", "layout.plain-list", "layout.toolbar",
            "menu.context", "menu.current", "menu.danger", "menu.detail", "menu.disabled", "menu.heading",
            "menu.note",
            "menu.row-actions", "menu.separator", "menu.split-button", "menu.tight", "menu.widths",
            "notices.empty-state", "notices.notice",
            "pager.edges", "pager.states", "pager.summary",
            "popover.match-anchor",
            "skeleton.group", "skeleton.row", "skeleton.short", "skeleton.spinner",
            "utilities.clamp", "utilities.focus-ring", "utilities.phone-only",
            "utilities.scroll-area",
            )"#,
        )
        // Switches (ui::Button with `checked`) are ghost buttons: a field's
        // ground and edge. Quiet ones are sockel's close buttons: filled,
        // their icon muted.
        .with(
            "button-colours",
            r#"(
            secondary: (fill: var(--neutral-bg), edge: transparent, pressed-fill: var(--button-fill), pressed-edge: var(--button-edge)),
            ghost: (fill: v.$bg-surface, ink: var(--text), edge: v.$border-input, hover-edge: var(--accent)),
            quiet: (fill: var(--neutral-bg), ink: var(--text-muted), edge: transparent),
            primary: (fill: var(--accent-bg), ink: var(--text), edge: transparent, hover-edge: var(--accent)),
            success: (fill: var(--success-bg), ink: var(--text), edge: transparent, hover-edge: var(--accent)),
            danger: (fill: var(--danger-bg), ink: var(--text), edge: transparent, hover-edge: var(--accent)),
            )"#,
        )
        .compile();
}
