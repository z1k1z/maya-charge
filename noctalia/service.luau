-- One asynchronous reader shared by all monitors; no overlapping USB queries.
local busy = false
local last = nil
noctalia.setUpdateInterval(5000)

local function publish(data)
    -- Do not notify widgets (or repaint) when the visible result is unchanged.
    if last and last.text == data.text and last.glyph == data.glyph
        and last.color == data.color and last.tooltip == data.tooltip then
        return
    end
    last = data
    noctalia.state.set("battery", data)
end

local function failed(message)
    publish({
        text = "—", glyph = "mouse-off", color = "on_surface_variant",
        tooltip = "LAMZU Maya\n" .. message,
    })
end

function update()
    if busy then return end
    busy = true
    local accepted = noctalia.runAsync({noctalia.pluginDir() .. "/mouse-charge", "--json"}, function(result)
        busy = false
        local ok, data = pcall(noctalia.json.decode, result.stdout)
        if result.timedOut or result.stdoutTruncated or not ok or type(data) ~= "table"
            or type(data.text) ~= "string" or type(data.glyph) ~= "string"
            or type(data.color) ~= "string" or type(data.tooltip) ~= "string" then
            failed("Не удалось прочитать заряд. Запусти mouse-charge --diagnose")
            return
        end
        publish(data)
    end)
    if not accepted then
        busy = false
        failed("Не удалось запустить mouse-charge")
    end
end

noctalia.state.watch("refresh", function() update() end)
update()
