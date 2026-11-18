-- Event-driven UI: no update() timer and no extra reader per monitor.
local shown = {}
local function render()
    local data = noctalia.state.get("battery") or {
        text = "—", glyph = "mouse", color = "secondary",
        tooltip = "LAMZU Maya: ожидание данных",
    }
    if shown.text ~= data.text then barWidget.setText(data.text) end
    if shown.glyph ~= data.glyph then barWidget.setGlyph(data.glyph) end
    if shown.color ~= data.color then
        barWidget.setColor(data.color)
        barWidget.setGlyphColor(data.color)
    end
    if shown.tooltip ~= data.tooltip then barWidget.setTooltip(data.tooltip) end
    shown = data
end

function onClick()
    noctalia.state.set("refresh", (noctalia.state.get("refresh") or 0) + 1)
end

noctalia.state.watch("battery", render)
render()
