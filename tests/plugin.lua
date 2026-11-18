-- Noctalia API double: lifecycle, asynchronous queries and repaint accounting.
local values, watchers, presentation = {}, {}, {}
local pending, calls, publications, setters = nil, 0, 0, 0
local launch = true
local intervals = {}
local function environment()
    local env = setmetatable({}, {__index = _G})
    env.noctalia = {
        setUpdateInterval = function(ms) table.insert(intervals, ms) end,
        pluginDir = function() return '/test/plugin' end,
        json = {decode = function(s)
            if s == 'bad' then error('bad JSON') end
            return s
        end},
        runAsync = function(argv, cb)
            assert(argv[1] == '/test/plugin/mouse-charge' and argv[2] == '--json')
            calls = calls + 1
            pending = cb
            return launch
        end,
        state = {
            get = function(k) return values[k] end,
            set = function(k,v)
                values[k] = v
                if k == 'battery' then publications = publications + 1 end
                for _,cb in ipairs(watchers[k] or {}) do cb(v) end
            end,
            watch = function(k,cb)
                watchers[k] = watchers[k] or {}
                table.insert(watchers[k], cb)
            end,
        },
    }
    env.barWidget = {}
    for _,key in ipairs({'Text','Glyph','Color','GlyphColor','Tooltip'}) do
        env.barWidget['set'..key] = function(value)
            setters = setters + 1
            presentation[key] = value
        end
    end
    return env
end
local function load(name)
    local env = environment()
    assert(loadfile('noctalia/'..name..'.luau','t',env))()
    return env
end
local function good(tooltip)
    return {stdout={text='87%',glyph='mouse',color='secondary',tooltip=tooltip or '87 percent'}}
end
local service = load('service')
local widget = load('widget')
assert(#intervals == 1 and intervals[1] == 5000, 'Only the service polls, every 5 s')
assert(rawget(widget,'update') == nil, 'Widget must not have a periodic callback')
assert(calls == 1 and presentation.Text == '—')
widget.onClick()
service.update()
assert(calls == 1, 'Do not overlap requests')
pending(good())
assert(presentation.Text == '87%')
local second = load('widget')
assert(calls == 1 and #intervals == 1, 'Second monitor shares reader and timer')
local previousPublications, previousSetters = publications, setters
service.update()
pending(good())
assert(publications == previousPublications and setters == previousSetters, 'Same battery must not redraw')
service.update()
pending(good('4100 mV'))
assert(setters == previousSetters + 2, 'Voltage-only changes update only the two tooltips')
second.onClick()
pending({stdout='bad'})
assert(presentation.Text == '—', 'Errors clear stale percentage')
widget.onClick()
pending({stdout={text='87%'}})
assert(presentation.Text == '—', 'Reject incomplete JSON')
widget.onClick()
pending({stdout={text='—',glyph='mouse-off',color='on_surface_variant',tooltip='No access'},exitCode=1})
assert(presentation.Tooltip == 'No access', 'Keep useful errors from CLI')
widget.onClick()
pending({stdout=good().stdout,timedOut=true})
assert(presentation.Text == '—', 'Timeout clears percentage')
launch = false
widget.onClick()
assert(presentation.Text == '—')
launch = true
widget.onClick()
pending(good())
assert(presentation.Text == '87%', 'Recover from failed process launch')
-- Noctalia restart: state and watches disappear; top-level code queries again.
values, watchers, intervals = {}, {}, {}
local beforeRestart = calls
service = load('service')
widget = load('widget')
assert(calls == beforeRestart + 1 and presentation.Text == '—')
pending(good())
assert(presentation.Text == '87%' and intervals[1] == 5000)
print('Plugin checks passed: 5 s, no overlap, shared reader, no redundant redraws, errors, restart')
