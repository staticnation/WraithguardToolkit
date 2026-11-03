-- Wraithguard: OpenMW Lua in the Preview, the Lua side (viewcore::luarun runs it).
--
-- The load order's scripts and OpenMW's own built-ins (resources/vfs, vfs-mw) run against a
-- copy of the scene the page keeps here: each frame `__wg_step` takes what changed (JSON)
-- and answers what the scripts did (JSON). Nothing here calls out of Lua, so it runs the
-- same on the install's LuaJIT and on Lua 5.4, and a script's error stays a Lua error.
--
-- Written for this page from OpenMW's API documentation and its resources/lua_api; the
-- engine-side functions OpenMW's built-ins call (`self:_startAiTravel`, `world.
-- _runStandardActivationAction`, ...) are answered from the scene. Lua 5.1 as LuaJIT
-- reads it. Copyright (c) 2026 StaticNation, GPL-2.0 as part of the viewer.

local G = _G
-- The engine's ray against the scene's collision (luarun.rs), the one call out of Lua.
local RAY = __wg_ray
__wg_ray = nil
local unpack = unpack or table.unpack
local floor, huge = math.floor, math.huge
local atan2 = math.atan2 or math.atan

------------------------------------------------------------------------------- JSON
local json = {}
do
  local ARRAY = {}
  json.array = function(t) return setmetatable(t or {}, ARRAY) end
  local esc = { ['"'] = '\\"', ['\\'] = '\\\\', ['\b'] = '\\b', ['\f'] = '\\f', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t' }
  local function str(s) return '"' .. s:gsub('[%c"\\]', function(c) return esc[c] or string.format('\\u%04x', c:byte()) end) .. '"' end
  local enc
  enc = function(v, out, depth)
    local t = type(v)
    if v == nil then out[#out + 1] = 'null'
    elseif t == 'boolean' then out[#out + 1] = v and 'true' or 'false'
    elseif t == 'number' then
      if v ~= v or v == huge or v == -huge then out[#out + 1] = 'null'
      elseif v == floor(v) and v > -1e15 and v < 1e15 then out[#out + 1] = string.format('%d', v)
      else out[#out + 1] = string.format('%.10g', v) end
    elseif t == 'string' then out[#out + 1] = str(v)
    elseif t == 'table' then
      if depth > 12 then out[#out + 1] = 'null' return end
      if getmetatable(v) == ARRAY or v[1] ~= nil then
        out[#out + 1] = '['
        for i = 1, #v do if i > 1 then out[#out + 1] = ',' end enc(v[i], out, depth + 1) end
        out[#out + 1] = ']'
      else
        out[#out + 1] = '{'
        local first = true
        for k, x in pairs(v) do
          if type(k) == 'string' or type(k) == 'number' then
            if not first then out[#out + 1] = ',' end
            first = false
            out[#out + 1] = str(tostring(k)); out[#out + 1] = ':'; enc(x, out, depth + 1)
          end
        end
        out[#out + 1] = '}'
      end
    else out[#out + 1] = 'null' end
  end
  json.encode = function(v) local out = {} enc(v, out, 0) return table.concat(out) end

  local function utf8(n)
    if n < 0x80 then return string.char(n) end
    if n < 0x800 then return string.char(0xC0 + floor(n / 64), 0x80 + n % 64) end
    return string.char(0xE0 + floor(n / 4096), 0x80 + floor(n / 64) % 64, 0x80 + n % 64)
  end
  json.decode = function(s)
    local i = 1
    local function ws() i = s:find('[^ \t\r\n]', i) or #s + 1 end
    local val
    local function strv()
      local out, j = {}, i + 1
      while true do
        local c = s:sub(j, j)
        if c == '"' then i = j + 1 return table.concat(out) end
        if c == '\\' then
          local e = s:sub(j + 1, j + 1)
          if e == 'u' then out[#out + 1] = utf8(tonumber(s:sub(j + 2, j + 5), 16)) j = j + 6
          else out[#out + 1] = ({ b = '\b', f = '\f', n = '\n', r = '\r', t = '\t' })[e] or e j = j + 2 end
        elseif c == '' then error('JSON: unterminated string')
        else
          local k = s:find('["\\]', j) or #s + 1
          out[#out + 1] = s:sub(j, k - 1) j = k
        end
      end
    end
    val = function()
      ws()
      local c = s:sub(i, i)
      if c == '{' then
        local t = {} i = i + 1 ws()
        if s:sub(i, i) == '}' then i = i + 1 return t end
        while true do
          ws() local k = strv() ws() i = i + 1
          t[k] = val() ws()
          local d = s:sub(i, i) i = i + 1
          if d == '}' then return t end
        end
      elseif c == '[' then
        local t = json.array() i = i + 1 ws()
        if s:sub(i, i) == ']' then i = i + 1 return t end
        while true do
          t[#t + 1] = val() ws()
          local d = s:sub(i, i) i = i + 1
          if d == ']' then return t end
        end
      elseif c == '"' then return strv()
      elseif s:sub(i, i + 3) == 'true' then i = i + 4 return true
      elseif s:sub(i, i + 4) == 'false' then i = i + 5 return false
      elseif s:sub(i, i + 3) == 'null' then i = i + 4 return nil
      else
        local a, b = s:find('^-?%d+%.?%d*[eE]?[-+]?%d*', i)
        if not a then error('JSON: unexpected ' .. c .. ' at ' .. i) end
        i = b + 1
        return tonumber(s:sub(a, b))
      end
    end
    return val()
  end
end

------------------------------------------------------------------------- the scene
-- What the page sent (kept between frames), and what the scripts did this frame.
local S = {
  objects = {}, player = { pos = { 0, 0, 0 }, yaw = 0, cell = { name = '', ext = true, gx = 0, gy = 0 } },
  hour = 12, day = 16, month = 7, year = 427, weather = 0, weathers = {}, globals = {}, actors = {},
  sounds = {}, musicPlaying = false, mwlocals = {}, files = {}, sim = 0, real = 0, dt = 0, journal = {},
}
local OUT, LOG = {}, {}
local current          -- the script running now
local function emit(e) OUT[#OUT + 1] = e end
local said = {}
local function log(kind, path, text) LOG[#LOG + 1] = { kind = kind, path = path or (current and current.path) or '', text = tostring(text) } end
local function once(key, kind, text) if not said[key] then said[key] = true log(kind, nil, text) end end
local function obj_of(key) return key == 'player' and S.player or S.objects[key] end

---------------------------------------------------------------------------- util
local util = {}
local V3, V2, T = {}, {}, {}
V3.__index, V2.__index, T.__index = V3, V2, T
local function v3(x, y, z) return setmetatable({ x = x or 0, y = y or 0, z = z or 0 }, V3) end
local function v2(x, y) return setmetatable({ x = x or 0, y = y or 0 }, V2) end
V3.__add = function(a, b) return v3(a.x + b.x, a.y + b.y, a.z + b.z) end
V3.__sub = function(a, b) return v3(a.x - b.x, a.y - b.y, a.z - b.z) end
V3.__mul = function(a, b)
  if type(a) == 'number' then return v3(b.x * a, b.y * a, b.z * a) end
  if type(b) == 'number' then return v3(a.x * b, a.y * b, a.z * b) end
  return a.x * b.x + a.y * b.y + a.z * b.z
end
V3.__div = function(a, b) return v3(a.x / b, a.y / b, a.z / b) end
V3.__unm = function(a) return v3(-a.x, -a.y, -a.z) end
V3.__eq = function(a, b) return a.x == b.x and a.y == b.y and a.z == b.z end
V3.__tostring = function(a) return string.format('(%g, %g, %g)', a.x, a.y, a.z) end
function V3:length() return math.sqrt(self.x * self.x + self.y * self.y + self.z * self.z) end
function V3:length2() return self.x * self.x + self.y * self.y + self.z * self.z end
function V3:normalize() local l = self:length() if l == 0 then return v3(0, 0, 0), 0 end return self / l, l end
function V3:dot(b) return self.x * b.x + self.y * b.y + self.z * b.z end
function V3:cross(b) return v3(self.y * b.z - self.z * b.y, self.z * b.x - self.x * b.z, self.x * b.y - self.y * b.x) end
function V3:ewMul(b) return v3(self.x * b.x, self.y * b.y, self.z * b.z) end
function V3:ewDiv(b) return v3(self.x / b.x, self.y / b.y, self.z / b.z) end
V3.emul, V3.ediv = V3.ewMul, V3.ewDiv
function V3:xy() return v2(self.x, self.y) end
V2.__add = function(a, b) return v2(a.x + b.x, a.y + b.y) end
V2.__sub = function(a, b) return v2(a.x - b.x, a.y - b.y) end
V2.__mul = function(a, b)
  if type(a) == 'number' then return v2(b.x * a, b.y * a) end
  if type(b) == 'number' then return v2(a.x * b, a.y * b) end
  return a.x * b.x + a.y * b.y
end
V2.__div = function(a, b) return v2(a.x / b, a.y / b) end
V2.__unm = function(a) return v2(-a.x, -a.y) end
V2.__eq = function(a, b) return a.x == b.x and a.y == b.y end
V2.__tostring = function(a) return string.format('(%g, %g)', a.x, a.y) end
function V2:length() return math.sqrt(self.x * self.x + self.y * self.y) end
function V2:length2() return self.x * self.x + self.y * self.y end
function V2:normalize() local l = self:length() if l == 0 then return v2(0, 0), 0 end return self / l, l end
function V2:dot(b) return self.x * b.x + self.y * b.y end
function V2:rotate(a) local c, s = math.cos(a), math.sin(a) return v2(self.x * c - self.y * s, self.x * s + self.y * c) end
util.vector3, util.vector2 = v3, v2
util.vector4 = function(x, y, z, w) return { x = x, y = y, z = z, w = w } end
util.clamp = function(v, lo, hi) return v < lo and lo or (v > hi and hi or v) end
util.round = function(v) return v >= 0 and floor(v + 0.5) or -floor(-v + 0.5) end
util.remap = function(v, a, b, c, d) return c + (v - a) * (d - c) / (b - a) end
util.normalizeAngle = function(a) local f = a / (2 * math.pi) + 0.5 return (f - floor(f) - 0.5) * 2 * math.pi end
util.color = {
  rgb = function(r, g, b) return { r = r, g = g, b = b, a = 1 } end,
  rgba = function(r, g, b, a) return { r = r, g = g, b = b, a = a } end,
  hex = function(h) local n = tonumber(h, 16) or 0 return { r = floor(n / 65536) % 256 / 255, g = floor(n / 256) % 256 / 255, b = n % 256 / 255, a = 1 } end,
  commaString = function(s)
    local r, g, b = tostring(s or ''):match('(%d+)%s*,%s*(%d+)%s*,%s*(%d+)')
    return { r = (tonumber(r) or 255) / 255, g = (tonumber(g) or 255) / 255, b = (tonumber(b) or 255) / 255, a = 1 }
  end,
}
-- Transforms: 3x3 rows and a move. rotateZ(a) turns +y toward +x, as Morrowind's yaw.
local function tf(m, o) return setmetatable({ m = m, o = o or v3(0, 0, 0) }, T) end
local function rotm(axis, a)
  local c, s = math.cos(a), math.sin(a)
  if axis == 'x' then return { 1, 0, 0, 0, c, -s, 0, s, c } end
  if axis == 'y' then return { c, 0, s, 0, 1, 0, -s, 0, c } end
  return { c, s, 0, -s, c, 0, 0, 0, 1 }
end
local function mm(a, b)
  local r = {}
  for i = 0, 2 do for j = 0, 2 do r[i * 3 + j + 1] = a[i * 3 + 1] * b[j + 1] + a[i * 3 + 2] * b[3 + j + 1] + a[i * 3 + 3] * b[6 + j + 1] end end
  return r
end
local function mv(m, v) return v3(m[1] * v.x + m[2] * v.y + m[3] * v.z, m[4] * v.x + m[5] * v.y + m[6] * v.z, m[7] * v.x + m[8] * v.y + m[9] * v.z) end
T.__mul = function(a, b)
  if getmetatable(b) == T then return tf(mm(a.m, b.m), mv(a.m, b.o) + a.o) end
  return mv(a.m, b) + a.o
end
function T:apply(v) return self * v end
function T:inverse() local m = self.m local t = { m[1], m[4], m[7], m[2], m[5], m[8], m[3], m[6], m[9] } return tf(t, -mv(t, self.o)) end
function T:getYaw() return atan2(self.m[2], self.m[5]) end
function T:getPitch() return math.asin(util.clamp(-self.m[8], -1, 1)) end
function T:getAnglesZYX() return self:getYaw(), self:getPitch(), 0 end
local ID = { 1, 0, 0, 0, 1, 0, 0, 0, 1 }
util.transform = {
  identity = tf(ID),
  move = function(x, y, z) if type(x) == 'table' then return tf(ID, v3(x.x, x.y, x.z)) end return tf(ID, v3(x, y, z)) end,
  scale = function(x, y, z) if type(x) == 'table' then x, y, z = x.x, x.y, x.z end return tf({ x, 0, 0, 0, y or x, 0, 0, 0, z or x }) end,
  rotateX = function(a) return tf(rotm('x', a)) end,
  rotateY = function(a) return tf(rotm('y', a)) end,
  rotateZ = function(a) return tf(rotm('z', a)) end,
  rotate = function(a, axis) return tf(rotm('z', a)) end,
}
util.makeReadOnly = function(t) return setmetatable({}, { __index = t, __newindex = function() error('read-only table', 2) end }) end
util.makeStrictReadOnly = util.makeReadOnly
util.loadCode = function(code, env) local f, e = loadstring(code) if f and setfenv then setfenv(f, env) end return f, e end

------------------------------------------------------------------------- objects
local objects = {}                -- key -> handle
local Obj = {}
local TYPES, types = {}, {}
local function handle(key) if key == nil then return nil end local o = objects[key] if not o then o = setmetatable({ __key = key }, Obj) objects[key] = o end return o end
local function keyOf(o) return type(o) == 'table' and rawget(o, '__key') or nil end
local function data(o) return obj_of(keyOf(o)) end
local function needGlobal(what)
  if current and current.ctx ~= 'global' then error(what .. ' can only be used from a global script', 3) end
end
local Cell = {}
Cell.__index = Cell
local function cellOf(c)
  if not c then return nil end
  local ext = c.ext ~= false
  return setmetatable({ name = c.name or '', id = ext and ('#' .. (c.gx or 0) .. ',' .. (c.gy or 0)) or string.lower(c.name or ''), isExterior = ext,
    gridX = c.gx or 0, gridY = c.gy or 0, region = c.region or '', hasWater = true, hasSky = ext, worldSpaceId = ext and 'sys::default' or string.lower(c.name or '') }, Cell)
end
Cell.__eq = function(a, b) return a.id == b.id end
function Cell:hasTag(t) return false end
function Cell:isInSameSpace(o) local c = o.cell return c ~= nil and (c.isExterior and self.isExterior or c.id == self.id) end
function Cell:getAll(t)
  local out = {}
  for k, d in pairs(S.objects) do
    local c = d.cell
    if c and not d.gone and ((self.isExterior and c.ext ~= false and c.gx == self.gridX and c.gy == self.gridY) or (not self.isExterior and c.ext == false and string.lower(c.name or '') == self.id)) then
      local o = handle(k)
      if not t or t.objectIsInstance(o) then out[#out + 1] = o end
    end
  end
  return out
end

local props = {
  position = function(d) local p = d.pos or { 0, 0, 0 } return v3(p[1], p[2], p[3]) end,
  rotation = function(d) local r = d.rot or { 0, 0, d.yaw or 0 } return util.transform.rotateZ(r[3] or 0) * util.transform.rotateY(r[2] or 0) * util.transform.rotateX(r[1] or 0) end,
  scale = function(d) return d.scale or 1 end,
  recordId = function(d, k) return k == 'player' and 'player' or d.id end,
  id = function(d, k) return k end,
  enabled = function(d) return d.enabled ~= false and not d.gone end,
  count = function(d) return d.count or 1 end,
  cell = function(d) return cellOf(d.cell) end,
  type = function(d, k) return TYPES[k == 'player' and 'PLAYER' or (d.tag or '')] or TYPES[''] end,
  contentFile = function() return nil end,
  parentContainer = function() return nil end,
  startingPosition = function(d) local p = d.start or d.pos or { 0, 0, 0 } return v3(p[1], p[2], p[3]) end,
  globalVariable = function() return nil end,
  ownerFactionId = function() return nil end,
}
Obj.__index = function(o, name)
  local k = rawget(o, '__key')
  local p = props[name]
  if p then return p(obj_of(k) or {}, k) end
  return Obj[name]
end
Obj.__newindex = function(o, name, v)
  if name == 'enabled' then
    needGlobal('enabled')
    local k = rawget(o, '__key')
    local d = obj_of(k)
    if d then d.enabled = v and true or false end
    emit({ k = 'enable', key = k, on = v and true or false })
    return
  end
  error('cannot set ' .. tostring(name) .. ' on a game object', 2)
end
Obj.__eq = function(a, b) return rawget(a, '__key') == rawget(b, '__key') end
Obj.__tostring = function(o) local d = data(o) or {} return 'object ' .. tostring(rawget(o, '__key')) .. ' (' .. tostring(d.id) .. ')' end
function Obj:isValid() local d = data(self) return d ~= nil and not d.gone end
function Obj:teleport(cell, pos, opt)
  needGlobal('teleport')
  local k = rawget(self, '__key')
  local name = type(cell) == 'table' and cell.name or cell
  local ext = type(cell) == 'table' and cell.isExterior or (name == nil or name == '')
  local rz
  if type(opt) == 'table' and opt.rotation then rz = opt.rotation:getYaw() elseif getmetatable(opt) == T then rz = opt:getYaw() end
  local d = obj_of(k)
  if not d then return end
  if k == 'player' then emit({ k = 'teleportPlayer', cell = name, pos = { pos.x, pos.y, pos.z } }) return end
  local gx, gy = floor(pos.x / 8192), floor(pos.y / 8192)
  d.pos = { pos.x, pos.y, pos.z }
  if rz then d.rot = { 0, 0, rz } end
  d.cell = { name = name or '', ext = ext, gx = gx, gy = gy }
  if d.loose then
    d.loose = nil
    emit({ k = 'create', key = k, id = d.id, cell = ext and '' or name, pos = d.pos, rz = rz or 0, count = d.count })
  else
    emit({ k = 'place', key = k, pos = d.pos, rz = rz })
  end
end
function Obj:setScale(s) needGlobal('setScale') local k = rawget(self, '__key') local d = obj_of(k) if d then d.scale = s end emit({ k = 'place', key = k, scale = s }) end
function Obj:remove(n) needGlobal('remove') local k = rawget(self, '__key') local d = obj_of(k) if d then d.gone = true end emit({ k = 'remove', key = k }) end
function Obj:split(n) return self end
local inventoryOf
function Obj:moveInto(inv)
  needGlobal('moveInto')
  local k = rawget(self, '__key')
  local d = obj_of(k)
  if not d then return end
  local owner = rawget(inv, '__owner')
  local a = S.actors[owner] or {}
  S.actors[owner] = a
  a.items = a.items or {}
  a.items[d.id] = (a.items[d.id] or 0) + (d.count or 1)
  emit({ k = 'inv', key = owner, id = d.id, n = d.count or 1 })
  if not d.loose then emit({ k = 'remove', key = k }) end
  d.gone = true
end
local activate
function Obj:activateBy(actor) activate(rawget(self, '__key'), keyOf(actor) or 'player') end
local sendLocal
function Obj:sendEvent(name, d) sendLocal(rawget(self, '__key'), name, d) end
local addScript, hasScript, removeScript
function Obj:addScript(path, d) needGlobal('addScript') addScript(path, rawget(self, '__key'), d) end
function Obj:hasScript(path) return hasScript(path, rawget(self, '__key')) end
function Obj:removeScript(path) needGlobal('removeScript') removeScript(path, rawget(self, '__key')) end
function Obj:setSaveState() end
function Obj:getBoundingBox() local p = props.position(data(self) or {}) return { center = p, halfSize = v3(64, 64, 64), vertices = {} } end

----------------------------------------------------------------------------- types
local function typeOf(name, tags)
  local t = { name = name }
  t.objectIsInstance = function(o)
    local k = keyOf(o)
    local tg = k == 'player' and 'PLAYER' or ((obj_of(k) or {}).tag or '')
    for _, x in ipairs(tags) do if x == tg then return true end end
    return false
  end
  t.record = function(o) local id = type(o) == 'string' and o or o.recordId return id and { id = id, name = id } or nil end
  t.records = {}
  return t
end
types.NPC = typeOf('NPC', { 'NPC_' })
types.Creature = typeOf('Creature', { 'CREA' })
types.Player = typeOf('Player', { 'PLAYER' })
types.Actor = typeOf('Actor', { 'NPC_', 'CREA', 'PLAYER' })
types.Activator = typeOf('Activator', { 'ACTI' })
types.Container = typeOf('Container', { 'CONT' })
types.Door = typeOf('Door', { 'DOOR' })
types.Light = typeOf('Light', { 'LIGH' })
types.Static = typeOf('Static', { 'STAT' })
types.Armor = typeOf('Armor', { 'ARMO' })
types.Book = typeOf('Book', { 'BOOK' })
types.Clothing = typeOf('Clothing', { 'CLOT' })
types.Ingredient = typeOf('Ingredient', { 'INGR' })
types.Miscellaneous = typeOf('Miscellaneous', { 'MISC' })
types.Potion = typeOf('Potion', { 'ALCH' })
types.Weapon = typeOf('Weapon', { 'WEAP' })
types.Apparatus = typeOf('Apparatus', { 'APPA' })
types.Lockpick = typeOf('Lockpick', { 'LOCK' })
types.Probe = typeOf('Probe', { 'PROB' })
types.Repair = typeOf('Repair', { 'REPA' })
types.Item = typeOf('Item', { 'ARMO', 'BOOK', 'CLOT', 'INGR', 'MISC', 'ALCH', 'WEAP', 'APPA', 'LOCK', 'PROB', 'REPA', 'LIGH' })
types.LevelledCreature = typeOf('LevelledCreature', {})
types.LevelledCreature.record = function() return nil end
types.ESM4Book = typeOf('ESM4Book', {})
types.ESM4Door = typeOf('ESM4Door', {})
TYPES = { NPC_ = types.NPC, CREA = types.Creature, PLAYER = types.Player, ACTI = types.Activator, CONT = types.Container, DOOR = types.Door,
  LIGH = types.Light, STAT = types.Static, ARMO = types.Armor, BOOK = types.Book, CLOT = types.Clothing, INGR = types.Ingredient,
  MISC = types.Miscellaneous, ALCH = types.Potion, WEAP = types.Weapon, APPA = types.Apparatus, LOCK = types.Lockpick, PROB = types.Probe,
  REPA = types.Repair, [''] = typeOf('Unknown', {}) }

local function actor(o) local k = keyOf(o) local a = S.actors[k] if not a then a = { hp = 100, mp = 100, fp = 100, max = { 100, 100, 100 }, items = {} } S.actors[k] = a end return a, k end
local Inv = {}
Inv.__index = Inv
inventoryOf = function(o) return setmetatable({ __owner = keyOf(o) }, Inv) end
function Inv:countOf(id) local a = S.actors[rawget(self, '__owner')] return a and a.items and a.items[string.lower(id)] or 0 end
function Inv:find(id) local n = self:countOf(id) if n > 0 then return { recordId = string.lower(id), count = n } end return nil end
function Inv:findAll(id) local f = self:find(id) return f and { f } or {} end
function Inv:getAll() local out = {} local a = S.actors[rawget(self, '__owner')] for id, n in pairs(a and a.items or {}) do out[#out + 1] = { recordId = id, count = n } end return out end
function Inv:isResolved() return true end
function Inv:resolve() end
types.Actor.inventory, types.NPC.inventory, types.Creature.inventory, types.Player.inventory = inventoryOf, inventoryOf, inventoryOf, inventoryOf
types.Container.content, types.Container.inventory = inventoryOf, inventoryOf

local function dyn(which)
  return function(o)
    return setmetatable({}, {
      __index = function(_, f)
        local a = actor(o)
        local cur = ({ a.hp, a.mp, a.fp })[which + 1] or 0
        local base = (a.max or {})[which + 1] or cur
        if f == 'current' then return cur elseif f == 'base' or f == 'modified' then return base elseif f == 'modifier' then return 0 end
      end,
      __newindex = function(_, f, v)
        local a, k = actor(o)
        if current and current.ctx ~= 'global' and current.key ~= k then error('another object\'s stats can only be changed from a global script', 2) end
        local field = ({ 'hp', 'mp', 'fp' })[which + 1]
        if f == 'current' then a[field] = v emit({ k = 'dyn', key = k, which = which, cur = v })
        elseif f == 'base' then a.max = a.max or {} a.max[which + 1] = v emit({ k = 'dyn', key = k, which = which, base = v }) end
      end,
    })
  end
end
types.Actor.stats = { dynamic = { health = dyn(0), magicka = dyn(1), fatigue = dyn(2) }, attributes = {}, skills = {},
  level = function() return { current = 1, progress = 0 } end }
types.NPC.stats, types.Creature.stats, types.Player.stats = types.Actor.stats, types.Actor.stats, types.Actor.stats
-- The player's journal, as the Scripts panel keeps it (Journal / SetJournalIndex): each quest
-- by its id, its stage; a stage set here goes back to the panel.
local function quest(id)
  id = string.lower(tostring(id))
  local q = { id = id }
  q.addJournalEntry = function(self, stage) if type(self) ~= 'table' then stage = self end S.journal[id] = stage emit({ k = 'journal', id = id, v = stage }) end
  return setmetatable(q, {
    __index = function(_, k)
      local st = S.journal[id] or 0
      if k == 'stage' then return st elseif k == 'started' then return st > 0 elseif k == 'finished' then return false end
    end,
    __newindex = function(_, k, v)
      if k == 'stage' then S.journal[id] = v emit({ k = 'journal', id = id, v = v }) else rawset(q, k, v) end
    end,
  })
end
types.Player.quests = function()
  local l = {}
  for id in pairs(S.journal) do l[id] = quest(id) end
  return setmetatable(l, { __index = function(t, id) local q = quest(id) rawset(t, string.lower(tostring(id)), q) return q end })
end
local switches = {}
types.Player.CONTROL_SWITCH = { Controls = 'playercontrols', Fighting = 'playerfighting', Jumping = 'playerjumping', Looking = 'playerlooking',
  Magic = 'playermagic', ViewMode = 'playerviewswitch', VanityMode = 'vanitymode' }
types.Player.getControlSwitch = function(_, k) return switches[k] ~= false end
types.Player.setControlSwitch = function(_, k, v) switches[k] = v end
types.Player.isCharGenFinished = function() return true end
types.Player.isTeleportingEnabled = function() return true end
types.Player.setTeleportingEnabled = function() end
types.Player.getCrimeLevel = function() return 0 end
types.Player.getBirthSign = function() return '' end
types.Player.sendMenuEvent = function() end
types.Player.isWerewolf = function() return false end
types.Actor.isDead = function(o) return actor(o).dead or false end
types.Actor.isDeathFinished = types.Actor.isDead
types.Actor.isInActorsProcessingRange = function() return true end
types.Actor.canMove = function(o) return not actor(o).dead end
types.Actor.STANCE = { Nothing = 0, Weapon = 1, Spell = 2 }
types.Actor.getStance = function(o) return actor(o).combat and 1 or 0 end
types.Actor.activeEffects = function() return { getEffect = function() return { magnitude = 0 } end, remove = function() end, modify = function() end } end
types.Actor.activeSpells = function() return { isSpellActive = function() return false end, add = function() end, remove = function() end } end
types.Actor.spells = function() return { add = function() end, remove = function() end } end
types.Actor.getEquipment = function() return {} end
types.Actor.EQUIPMENT_SLOT = {}

-------------------------------------------------------------------------- scripts
local scripts = {}       -- the running instances, in start order
local byKey = {}         -- object key -> instances
local registered = {}    -- {path, flags} in load order (built-ins first)
local sources = {}       -- lowercased path -> text
local globalQueue, localQueue = {}, {}
local ifaceGlobal, ifaceLocal = {}, {}
local timers, named = {}, {}

local function call(inst, fn, ...)
  if type(fn) ~= 'function' then return true end
  local was = current
  current = inst
  local res = { pcall(fn, ...) }
  current = was
  if not res[1] then
    -- Once each: a handler that fails every frame is named once, not thirty times a second.
    local msg = tostring(res[2])
    local key = 'err|' .. inst.path .. '|' .. msg
    if not said[key] then said[key] = true log('error', inst.path, msg) end
    inst.errors = (inst.errors or 0) + 1
  end
  return unpack(res)
end
local function deliver(list, name, d)
  for i = #list, 1, -1 do
    local inst = list[i]
    local h = not inst.dead and inst.events[name]
    if h then
      local _, ret = call(inst, h, d)
      if ret == false then return end
    end
  end
end
sendLocal = function(key, name, d) localQueue[#localQueue + 1] = { key, name, d } end

---------------------------------------------------------------------------- packages
local packages = {}
local storage = {}
local function section(name)
  local s = storage[name]
  if s then return s end
  local d, subs = {}, {}
  s = {
    get = function(_, k) return d[k] end,
    getCopy = function(_, k) return d[k] end,
    set = function(_, k, v) d[k] = v for _, f in ipairs(subs) do pcall(f, name, k) end end,
    asTable = function() local t = {} for k, v in pairs(d) do t[k] = v end return t end,
    reset = function(_, t) d = t or {} end,
    subscribe = function(_, cb) subs[#subs + 1] = type(cb) == 'table' and cb.fn or cb end,
    setLifeTime = function() end,
  }
  storage[name] = s
  return s
end
local weatherRecs = {}
local made = 0         -- objects world.createObject made (their keys)
local function weatherRecord(i)
  local name = S.weathers[i + 1] or 'Clear'
  local r = weatherRecs[name]
  if not r then r = { recordId = string.lower(name), name = name, scriptId = i } weatherRecs[name] = r end
  return r
end

packages['openmw.util'] = function() return util end
packages['openmw.core'] = function(inst)
  local core = {
    API_REVISION = 100,
    getSimulationTime = function() return S.sim end,
    getSimulationTimeScale = function() return 1 end,
    getGameTime = function() return ((S.day - 1) + (S.month - 1) * 30) * 86400 + S.hour * 3600 end,
    getGameTimeScale = function() return 30 end,
    getRealTime = function() return S.real end,
    getRealFrameDuration = function() return S.dt end,
    getSimulationFrameDuration = function() return S.dt end,
    isWorldPaused = function() return false end,
    sendGlobalEvent = function(name, d) globalQueue[#globalQueue + 1] = { name, d } end,
    -- The game settings the built-ins read (the UI's colors), as Morrowind ships them.
    getGMST = function(k)
      local t = { FontColor_color_header = '223,201,159', FontColor_color_normal = '202,165,96', FontColor_color_normal_over = '223,201,159',
        FontColor_color_active = '96,112,202', FontColor_color_disabled = '179,168,135', FontColor_color_link = '112,126,207' }
      return t[k]
    end,
    contentFiles = { has = function() return true end, indexOf = function() return 1 end, list = {} },
    l10n = function() return function(k) return k end end,
    getFormId = function(file, id) return tostring(id) end,
    sound = {
      isEnabled = function() return true end,
      playSound3d = function(id, o, opt) local k = keyOf(o) S.sounds[k .. '|' .. string.lower(id)] = true emit({ k = 'sound3d', id = id, key = k, vol = opt and opt.volume, loop = opt and opt.loop }) end,
      playSoundFile3d = function(file, o, opt) emit({ k = 'say', file = file, key = keyOf(o) }) end,
      stopSound3d = function(id, o) local k = keyOf(o) S.sounds[k .. '|' .. string.lower(id)] = nil emit({ k = 'stop3d', id = id, key = k }) end,
      stopSoundFile3d = function() end,
      isSoundPlaying = function(id, o) return S.sounds[keyOf(o) .. '|' .. string.lower(id)] == true end,
      isSoundFilePlaying = function(file, o) return S.sounds[keyOf(o) .. '|*say'] == true end,
      say = function(file, o, text) local k = keyOf(o) S.sounds[k .. '|*say'] = true emit({ k = 'say', file = file, key = k, text = text }) end,
      stopSay = function() end,
      isSayActive = function(o) return S.sounds[keyOf(o) .. '|*say'] == true end,
    },
    weather = {
      getCurrent = function() return weatherRecord(S.weather) end,
      getNext = function() return nil end,
      getTransition = function() return 0 end,
      changeWeather = function(region, rec)
        if inst.ctx ~= 'global' then error('core.weather.changeWeather can only be used in global scripts', 2) end
        local name = type(rec) == 'table' and (rec.name or rec.recordId) or tostring(rec)
        for i, n in ipairs(S.weathers) do if string.lower(n) == string.lower(name) then S.weather = i - 1 end end
        emit({ k = 'weather', name = name })
      end,
      records = {},
    },
    magic = { EFFECT_TYPE = {}, RANGE = {}, effects = { records = {} }, spells = { records = {} } },
    stats = { Attribute = { records = {} }, Skill = { records = {} } },
    dialogue = { journal = { records = {} }, topic = { records = {} } },
    factions = { records = {} }, regions = { records = {} },
  }
  for i = 1, #S.weathers do core.weather.records[i] = weatherRecord(i - 1) end
  return core
end
packages['openmw.async'] = function(inst)
  local function at(delay, game) return S.sim + (game and delay / 30 or delay) end
  return {
    registerTimerCallback = function(_, name, fn) named[inst.path .. '|' .. name] = fn return { name = name, fn = fn } end,
    newSimulationTimer = function(_, delay, cb, arg) timers[#timers + 1] = { at = at(delay), cb = cb, arg = arg, inst = inst } end,
    newGameTimer = function(_, delay, cb, arg) timers[#timers + 1] = { at = at(delay, true), cb = cb, arg = arg, inst = inst } end,
    newUnsavableSimulationTimer = function(_, delay, fn) timers[#timers + 1] = { at = at(delay), cb = fn, inst = inst } end,
    newUnsavableGameTimer = function(_, delay, fn) timers[#timers + 1] = { at = at(delay, true), cb = fn, inst = inst } end,
    callback = function(_, fn) return fn end,
  }
end
packages['openmw.storage'] = function()
  return {
    globalSection = function(n) return section('g:' .. n) end,
    playerSection = function(n) return section('p:' .. n) end,
    allPlayers = function() return {} end,
    LIFE_TIME = { Persistent = 0, GameSession = 1, Temporary = 2 },
  }
end
packages['openmw.vfs'] = function()
  return {
    pathsWithPrefix = function(prefix)
      local p, i = string.lower(prefix), 0
      return function()
        while true do
          i = i + 1
          local f = S.files[i]
          if not f then return nil end
          if string.lower(f):sub(1, #p) == p then return f end
        end
      end
    end,
    fileExists = function(path) local p = string.lower(path) for _, f in ipairs(S.files) do if string.lower(f) == p then return true end end return sources[p] ~= nil end,
    open = function() return nil, 'files are not read in the preview' end,
    lines = function() return function() return nil end end,
  }
end
packages['openmw.debug'] = function() return { isAIEnabled = function() return true end, isCollisionEnabled = function() return true end, isGodMode = function() return false end, toggleAI = function() end, RENDER_MODE = {} } end
packages['openmw.markup'] = function() return { decodeYaml = function() return {} end, loadYaml = function() return {} end } end
packages['openmw.world'] = function(inst)
  if inst.ctx ~= 'global' then error('openmw.world is only for global scripts', 3) end
  local function actors() local l = {} for k, d in pairs(S.objects) do if (d.tag == 'NPC_' or d.tag == 'CREA') and not d.gone and d.enabled ~= false then l[#l + 1] = handle(k) end end l[#l + 1] = handle('player') return l end
  -- activeActors is a plain list made when read: LuaJIT's # does not ask a table's __len.
  local world = {
    players = { handle('player') },
    getCellByName = function(n)
      for _, d in pairs(S.objects) do local c = d.cell if c and string.lower(c.name or '') == string.lower(n) then return cellOf(c) end end
      return cellOf({ name = n, ext = false })
    end,
    getExteriorCell = function(x, y) return cellOf({ name = '', ext = true, gx = x, gy = y }) end,
    getCellById = function(n) return cellOf({ name = n, ext = false }) end,
    cells = {},
    createObject = function(id, n)
      made = made + 1
      local key = 'lua:' .. made
      S.objects[key] = { id = string.lower(id), count = n or 1, loose = true, enabled = true, tag = '' }
      return handle(key)
    end,
    getObjectByFormId = function() return nil end,
    getGameTime = function() return packages['openmw.core'](inst).getGameTime() end,
    getSimulationTime = function() return S.sim end,
    getGameTimeScale = function() return 30 end,
    setGameTimeScale = function() end,
    setSimulationTimeScale = function() end,
    isWorldPaused = function() return false end,
    pause = function() end, unpause = function() end,
    vfx = { spawn = function(model) log('do', inst.path, 'vfx ' .. tostring(model)) end, remove = function() end },
    _runStandardActivationAction = function(o, by) emit({ k = 'activate', key = keyOf(o), by = keyOf(by) }) end,
    mwscript = {
      getGlobalVariables = function()
        return setmetatable({}, {
          __index = function(_, n) local g = S.globals[string.lower(n)] if g then return g.v end if string.lower(n) == 'gamehour' then return S.hour end end,
          __newindex = function(_, n, v)
            local k = string.lower(n)
            if k == 'gamehour' then S.hour = v emit({ k = 'hour', v = v }) return end
            local g = S.globals[k]
            if not g then error('no global variable ' .. tostring(n), 2) end
            g.v = (g.kind == 'f') and v or (v >= 0 and floor(v) or -floor(-v))
            emit({ k = 'global', name = k, v = g.v })
          end,
        })
      end,
      getLocalScript = function(o)
        local k = keyOf(o)
        local m = S.mwlocals[k]
        if not m then return nil end
        return { recordId = m.script, object = o, variables = setmetatable({}, {
          __index = function(_, n) return m.vars[string.lower(n)] end,
          __newindex = function(_, n, v) m.vars[string.lower(n)] = v emit({ k = 'mwlocal', key = k, name = string.lower(n), v = v }) end }) }
      end,
      -- A global MWScript the Scripts panel runs (StartScript), with its locals; nil for
      -- one not started, as in OpenMW.
      getGlobalScript = function(id)
        local m = S.mwlocals['global:' .. string.lower(tostring(id))]
        if not m then return nil end
        local k = 'global:' .. string.lower(tostring(id))
        return { recordId = m.script, isRunning = true, variables = setmetatable({}, {
          __index = function(_, n) return m.vars[string.lower(n)] end,
          __newindex = function(_, n, v) m.vars[string.lower(n)] = v emit({ k = 'mwlocal', key = k, name = string.lower(n), v = v }) end }) }
      end,
    },
  }
  return setmetatable(world, { __index = function(_, k) if k == 'activeActors' then return actors() end end })
end
-- The actor functions OpenMW's own ai.lua calls (`self:_startAi...`), answered from the scene.
local function aiOf(key)
  local a = S.actors[key] or {}
  local p = a.ai
  if a.combat then return { type = 'Combat', target = handle(a.combat) } end
  if not p or p.done then return nil end
  local T = ({ wander = 'Wander', travel = 'Travel', follow = 'Follow', escort = 'Escort', activate = 'Activate' })[p.kind] or p.kind
  local d = p.dest
  return { type = T, target = p.target and handle(p.target) or nil, destPosition = d and v3(d[1], d[2], d[3]) or nil, distance = p.distance, duration = p.duration, isRepeat = false, sideWithTarget = false }
end
local function startAi(key, kind, o)
  local a = S.actors[key] or { hp = 100, mp = 100, fp = 100, max = { 100, 100, 100 }, items = {} }
  S.actors[key] = a
  if kind == 'combat' then a.combat = o.target emit({ k = 'combat', key = key, target = o.target }) return end
  a.ai = { kind = kind, target = o.target, dest = o.dest, distance = o.distance, duration = o.duration, done = false }
  emit({ k = 'ai', key = key, kind = kind, opt = o })
end
local function selfInternals(key)
  local function tk(t) return keyOf(t) end
  local function dv(p) return p and { p.x, p.y, p.z } or nil end
  return {
    _startAiCombat = function(_, target) startAi(key, 'combat', { target = tk(target) }) end,
    _startAiPursue = function(_, target) startAi(key, 'follow', { target = tk(target) }) end,
    _startAiFollow = function(_, target, cellId, duration, dest) startAi(key, 'follow', { target = tk(target), duration = duration, dest = (dest and (dest.x ~= 0 or dest.y ~= 0 or dest.z ~= 0)) and dv(dest) or nil }) end,
    _startAiEscort = function(_, target, cell, duration, dest) startAi(key, 'escort', { target = tk(target), duration = duration, dest = dv(dest) }) end,
    _startAiWander = function(_, distance, duration) startAi(key, 'wander', { distance = distance, duration = duration }) end,
    _startAiTravel = function(_, dest) startAi(key, 'travel', { dest = dv(dest) }) end,
    _getActiveAiPackage = function() return aiOf(key) end,
    _isFleeing = function() return false end,
    _iterateAndFilterAiSequence = function(_, filter)
      local p = aiOf(key)
      if p and not filter(p) then
        local a = S.actors[key]
        if a then a.ai = nil a.combat = nil end
        emit({ k = 'aiClear', key = key })
      end
    end,
    _enableLuaAnimations = function() end,
  }
end
packages['openmw.self'] = function(inst)
  if inst.ctx == 'global' then error('openmw.self is only for local scripts', 3) end
  local o = handle(inst.key)
  local extra = selfInternals(inst.key)
  local s = setmetatable({ controls = { movement = 0, sideMovement = 0, yawChange = 0, pitchChange = 0, run = false, jump = false, sneak = false, use = 0 },
    object = o, enableAI = function() end, ATTACK_TYPE = {} }, {
    __index = function(_, n) if extra[n] then return extra[n] end local f = Obj[n] if type(f) == 'function' then return function(_, ...) return f(o, ...) end end return o[n] end,
    __newindex = function(t, n, v) if n == 'enabled' then error('a local script cannot disable its own object; send a global event', 2) end rawset(t, n, v) end,
    __eq = function(a, b) return keyOf(a) == keyOf(b) end,
    __tostring = function() return tostring(o) end,
  })
  rawset(s, '__key', inst.key)
  return s
end
-- nearby.castRay: the scene's collision shapes and the land; `ignore` an object (or a list).
local function castRay(from, to, opt)
  if not RAY then once('castRay', 'skip', 'nearby.castRay: no collision to cast against') return { hit = false } end
  local ig = opt and opt.ignore
  if type(ig) == 'table' and not rawget(ig, '__key') and ig[1] then ig = ig[1] end
  local x, y, z, nx, ny, nz, k = RAY(from.x, from.y, from.z, to.x, to.y, to.z, keyOf(ig))
  if not x then return { hit = false } end
  return { hit = true, hitPos = v3(x, y, z), hitNormal = v3(nx, ny, nz), hitObject = k and handle(k) or nil }
end
packages['openmw.nearby'] = function(inst)
  if inst.ctx == 'global' then error('openmw.nearby is only for local scripts', 3) end
  local kinds = { actors = { NPC_ = true, CREA = true }, items = { ARMO = true, BOOK = true, CLOT = true, INGR = true, MISC = true, ALCH = true, WEAP = true, APPA = true, LOCK = true, PROB = true, REPA = true, LIGH = true },
    doors = { DOOR = true }, activators = { ACTI = true }, containers = { CONT = true } }
  return setmetatable({
    castRay = function(from, to, opt) return castRay(from, to, opt) end,
    castRenderingRay = function(from, to, opt) return castRay(from, to, opt) end,
    players = { handle('player') },
    COLLISION_TYPE = { World = 1, Door = 2, Actor = 4, HeightMap = 8, Projectile = 16, Water = 32, Default = 31, AnyPhysical = 127, Camera = 64, VisualOnly = 128 },
    findPath = function() return 1, {} end,
    NAVIGATOR_FLAGS = {}, FIND_PATH_STATUS = { Success = 0 },
  }, { __index = function(_, n)
    local want = kinds[n]
    if not want then return nil end
    local out = {}
    for k, d in pairs(S.objects) do if want[d.tag or ''] and not d.gone and d.enabled ~= false then out[#out + 1] = handle(k) end end
    if n == 'actors' then out[#out + 1] = handle('player') end
    return out
  end })
end
packages['openmw.types'] = function() return types end
packages['openmw.ambient'] = function(inst)
  if inst.ctx ~= 'player' then error('openmw.ambient is only for player scripts', 3) end
  return {
    playSound = function(id, opt) emit({ k = 'sound', id = id, vol = opt and opt.volume }) end,
    playSoundFile = function(f) emit({ k = 'say', file = f, key = 'player' }) end,
    stopSound = function() end, stopSoundFile = function() end,
    isSoundPlaying = function(id) return S.sounds['player|' .. string.lower(id)] == true end,
    isSoundFilePlaying = function() return false end,
    streamMusic = function(path) S.musicPlaying = true emit({ k = 'music', path = path }) end,
    stopMusic = function() S.musicPlaying = false emit({ k = 'stopMusic' }) end,
    isMusicPlaying = function() return S.musicPlaying end,
    say = function(file, text) S.sounds['player|*say'] = true emit({ k = 'say', file = file, key = 'player', text = text }) end,
    stopSay = function() end, isSayActive = function() return S.sounds['player|*say'] == true end,
  }
end
-- ui.create: each element's layout, its templates resolved as OpenMW resolves them, sent
-- to the page to draw (`{k = 'ui', id, layer, tree}`); a click or a typed text comes back
-- and calls the element's own event callback.
local uiElements, uiNext = {}, 0        -- id -> element
local uiNodes = {}                      -- id -> {path -> layout node} (for the events)
local Content = {}
-- A content's children by name too (`layout.content.title`), as OpenMW's.
Content.__index = function(t, k) if Content[k] then return Content[k] end local i = type(k) == 'string' and rawget(t, '#' .. k) return i and rawget(t, i) or nil end
function Content:add(v) self[#self + 1] = v if type(v) == 'table' and v.name then rawset(self, '#' .. v.name, #self) end return #self end
function Content:insert(i, v) table.insert(self, i, v) end
function Content:indexOf(v) for i, x in ipairs(self) do if x == v or (type(v) == 'string' and type(x) == 'table' and x.name == v) then return i end end return nil end
local function uiContent(t)
  local c = setmetatable({}, Content)
  for _, v in ipairs(t or {}) do c:add(v) end
  return c
end
local function isElement(v) return type(v) == 'table' and rawget(v, '__wgElement') ~= nil end
local function plain(v)
  local mt = getmetatable(v)
  if mt == V2 or mt == V3 then return { x = v.x, y = v.y, z = v.z } end
  if type(v) == 'table' and v.__texture then return { path = v.path, offset = v.offset and plain(v.offset), size = v.size and plain(v.size) } end
  if type(v) == 'table' and v.r and v.g and v.b then return { r = v.r, g = v.g, b = v.b, a = v.a or 1 } end
  if type(v) == 'function' or type(v) == 'userdata' then return nil end
  if type(v) == 'table' then local o = {} for k, x in pairs(v) do if type(k) == 'string' then o[k] = plain(x) end end return o end
  return v
end
-- A node as drawn: its template under it (template props first, the node's own over them;
-- the node's content into the template's `external.slot`).
local resolve
resolve = function(node, nodes, path, depth)
  if depth > 40 then return nil end
  if isElement(node) then
    node.__parents = node.__parents or {}
    return resolve(node.layout or {}, nodes, path, depth + 1)
  end
  if type(node) ~= 'table' then return nil end
  local tpl = node.template
  local typ = node.type or (tpl and tpl.type) or 'Widget'
  local props = {}
  if tpl and tpl.props then for k, v in pairs(tpl.props) do props[k] = v end end
  for k, v in pairs(node.props or {}) do props[k] = v end
  local out = { type = typ, props = plain(props), name = node.name, path = path, events = json.array() }
  for k in pairs(node.events or {}) do out.events[#out.events + 1] = k end
  nodes[path] = node
  local kids = json.array()
  local function add(list, at)
    for i, c in ipairs(list or {}) do
      local r = resolve(c, nodes, path .. '.' .. at .. i, depth + 1)
      if r then kids[#kids + 1] = r end
    end
  end
  if tpl and tpl.content then
    -- The template's content, with this node's content where the template takes it.
    local function walk(list, at)
      for i, c in ipairs(list) do
        if type(c) == 'table' and c.external and c.external.slot then
          local r = resolve({ type = c.type, props = c.props, template = c.template, name = c.name }, nodes, path .. '.' .. at .. i, depth + 1)
          if r then
            local inner = json.array()
            for k, x in ipairs(node.content or {}) do local rr = resolve(x, nodes, path .. '.s' .. k, depth + 1) if rr then inner[#inner + 1] = rr end end
            r.content = inner
            kids[#kids + 1] = r
          end
        else
          local r = resolve(c, nodes, path .. '.' .. at .. i, depth + 1)
          if r then kids[#kids + 1] = r end
        end
      end
    end
    walk(tpl.content, 't')
  else
    add(node.content, 'c')
  end
  out.content = kids
  return out
end
local function uiSend(el)
  local nodes = {}
  local tree = resolve(el.layout or {}, nodes, 'r', 0)
  uiNodes[el.__id] = nodes
  emit({ k = 'ui', id = el.__id, layer = (el.layout and el.layout.layer) or 'Windows', tree = tree })
end
local Element = {}
Element.__index = Element
function Element:update() if not self.__gone then uiSend(self) end end
function Element:destroy() if not self.__gone then self.__gone = true uiElements[self.__id] = nil uiNodes[self.__id] = nil emit({ k = 'uiDestroy', id = self.__id }) end end

-- A click or typed text from the page: the node's own callback, as OpenMW calls it.
function __wg_ui_event(id, path, event, x, y, text)
  local node = uiNodes[id] and uiNodes[id][path]
  local cb = node and node.events and node.events[event]
  if not cb then return end
  local el = uiElements[id]
  local fn = type(cb) == 'table' and cb.fn or cb
  local arg = event == 'textChanged' and text or { position = v2(x or 0, y or 0), offset = v2(0, 0), button = 1 }
  call(el and el.__inst or { path = 'ui' }, fn, arg, node)
end

packages['openmw.ui'] = function(inst)
  if inst.ctx ~= 'player' then error('openmw.ui is only for player scripts', 3) end
  local any = setmetatable({}, { __index = function(_, k) return k end })
  return {
    showMessage = function(t) emit({ k = 'msg', text = tostring(t) }) end,
    printToConsole = function(t) log('do', inst.path, tostring(t)) end,
    setConsoleMode = function() end, setConsoleSelectedObject = function() end,
    create = function(layout)
      uiNext = uiNext + 1
      local el = setmetatable({ layout = layout, __id = 'e' .. uiNext, __inst = inst, __wgElement = true, __type = { name = 'LuaUi::Element' } }, Element)
      uiElements[el.__id] = el
      -- Only a top-level element is drawn by itself; one inside another's content is drawn
      -- with that one (and sent again when it is).
      if layout and layout.layer then uiSend(el) end
      return el
    end,
    updateAll = function() for _, el in pairs(uiElements) do if el.layout and el.layout.layer then uiSend(el) end end end,
    content = uiContent,
    TYPE = { Widget = 'Widget', Text = 'Text', TextEdit = 'TextEdit', Image = 'Image', Flex = 'Flex', Container = 'Container', Window = 'Window' },
    ALIGNMENT = { Start = 'Start', Center = 'Center', End = 'End' },
    CONSOLE_COLOR = any, WINDOW = any, MODE = any,
    texture = function(t) return { path = t.path, offset = t.offset, size = t.size, __texture = true } end,
    layers = setmetatable({ indexOf = function() return 1 end, insertAfter = function() end, insertBefore = function() end }, { __index = function() return nil end }),
    screenSize = function() return v2(S.screen and S.screen[1] or 1920, S.screen and S.screen[2] or 1080) end,
    _getMenuTransparency = function() return 1 end,
    _getDefaultFontSize = function() return 16 end,
    _setHudVisibility = function() end, _isHudVisible = function() return true end,
  }
end
packages['openmw.camera'] = function(inst)
  if inst.ctx ~= 'player' then error('openmw.camera is only for player scripts', 3) end
  -- The Preview's camera is the player's eyes: first person, OpenMW's defaults for the rest.
  -- What a script sets is kept for it to read back; the view itself stays the page's.
  local set = { tpd = 192, fov = math.rad(60), extra = { 0, 0 }, roll = 0, crosshair = true }
  local nop = function() end
  return { getPosition = function() local p = S.player.pos return v3(p[1], p[2], p[3] + 116) end, getYaw = function() return S.player.yaw or 0 end,
    getPitch = function() return 0 end, getRoll = function() return set.roll end, getMode = function() return 0 end, getQueuedMode = function() return nil end,
    MODE = { FirstPerson = 0, ThirdPerson = 1, Static = 2, Preview = 3, Vanity = 4 },
    getThirdPersonDistance = function() return set.tpd end, setPreferredThirdPersonDistance = function(d) set.tpd = d end,
    getFieldOfView = function() return set.fov end, getBaseFieldOfView = function() return math.rad(60) end, setFieldOfView = function(f) set.fov = f end,
    getViewDistance = function() return 8192 end, setViewDistance = nop, getBaseViewDistance = function() return 8192 end,
    getExtraPitch = function() return set.extra[1] end, getExtraYaw = function() return set.extra[2] end,
    setExtraPitch = function(v) set.extra[1] = v end, setExtraYaw = function(v) set.extra[2] = v end, setExtraRoll = nop, getExtraRoll = function() return 0 end,
    getTrackedPosition = function() local p = S.player.pos return v3(p[1], p[2], p[3] + 116) end,
    getFocalPreferredOffset = function() return v2(0, 0) end, setFocalPreferredOffset = nop, getFocalTransitionSpeed = function() return 1 end, setFocalTransitionSpeed = nop,
    instantTransition = nop, setMode = nop, setStaticPosition = nop, setYaw = nop, setPitch = nop, setRoll = nop,
    allowCharacterDeferredRotation = nop, showCrosshair = function(b) set.crosshair = b end, getCollisionType = function() return 0 end, setCollisionType = nop,
    viewportToWorldVector = function() local y = S.player.yaw or 0 return v3(math.sin(y), math.cos(y), 0) end,
    worldToViewportVector = function() return v3(0.5, 0.5, 1) end }
end
packages['openmw.input'] = function()
  local any = setmetatable({}, { __index = function(_, k) return k end })
  return { isKeyPressed = function() return false end, isActionPressed = function() return false end, getBooleanActionValue = function() return false end,
    getRangeActionValue = function() return 0 end, KEY = any, ACTION = any, CONTROLLER_BUTTON = any, CONTROLLER_AXIS = any,
    registerAction = function() end, registerActionHandler = function() end, registerTrigger = function() end, registerTriggerHandler = function() end,
    getMouseMoveX = function() return 0 end, getMouseMoveY = function() return 0 end, isIdle = function() return true end }
end
packages['openmw.interfaces'] = function(inst)
  return setmetatable({}, { __index = function(_, name)
    if inst.ctx == 'global' then return ifaceGlobal[name] end
    local l = ifaceLocal[inst.key]
    return l and l[name] or nil
  end })
end
local function knownPackage(name) return packages[name] ~= nil end

--------------------------------------------------------------------- the sandbox
local BASE = { 'assert', 'error', 'getmetatable', 'ipairs', 'next', 'pairs', 'pcall', 'rawequal', 'rawget', 'rawset', 'select',
  'setmetatable', 'tonumber', 'tostring', 'type', 'xpcall', 'coroutine', 'string', 'table', 'math', 'bit', 'unpack' }
local function loadIn(text, name, env)
  local f, err
  if setfenv and loadstring then
    f, err = loadstring(text, name)
    if f then setfenv(f, env) end
  else
    f, err = load(text, name, 't', env)
  end
  return f, err
end
local function sandbox(inst)
  local env = {}
  for _, n in ipairs(BASE) do env[n] = G[n] end
  env.unpack = unpack
  env.print = function(...) local t = {} for i = 1, select('#', ...) do t[i] = tostring((select(i, ...))) end log('print', inst.path, table.concat(t, ' ')) end
  env.os = { time = os.time, date = os.date, clock = os.clock, difftime = os.difftime }
  env._G = env
  local loaded = {}
  env.require = function(name)
    if loaded[name] ~= nil then return loaded[name] end
    local v
    if packages[name] then v = packages[name](inst)
    else
      local file = string.lower(name:gsub('%.', '/'))
      local text = sources[file .. '.lua'] or sources[file .. '/init.lua']
      if not text then
        if name:match('^openmw%.') then
          once('pkg:' .. name, 'skip', name .. ' is not run in the preview')
          v = setmetatable({}, { __index = function() return function() end end })
        else
          error('module ' .. name .. ' not found', 2)
        end
      else
        local f, err = loadIn(text, '@' .. file .. '.lua', env)
        if not f then error(err, 2) end
        v = f(name)
        if v == nil then v = true end
      end
    end
    loaded[name] = v
    return v
  end
  return env
end

local function instantiate(path, key, ctx, initData)
  local text = sources[string.lower((path:gsub('\\', '/')))]
  if not text then log('warn', path, 'not in the data folders') return nil end
  local inst = { path = path, key = key, ctx = ctx, handlers = {}, events = {} }
  inst.env = sandbox(inst)
  local f, err = loadIn(text, '@' .. path, inst.env)
  if not f then log('error', path, err) return nil end
  local ok, ret = call(inst, f)
  if not ok then return nil end
  if type(ret) ~= 'table' then ret = {} end
  inst.handlers = ret.engineHandlers or {}
  inst.events = ret.eventHandlers or {}
  if ret.interfaceName and ret.interface then
    if ctx == 'global' then ifaceGlobal[ret.interfaceName] = ret.interface
    else ifaceLocal[key] = ifaceLocal[key] or {} ifaceLocal[key][ret.interfaceName] = ret.interface end
  end
  scripts[#scripts + 1] = inst
  if key then byKey[key] = byKey[key] or {} table.insert(byKey[key], inst) end
  call(inst, inst.handlers.onInit, initData)
  if key and ctx ~= 'player' then call(inst, inst.handlers.onActive) end
  return inst
end
addScript = function(path, key, d) if instantiate(path, key, key == 'player' and 'player' or 'local', d) then log('do', current and current.path, 'addScript ' .. path) end end
hasScript = function(path, key) for _, i in ipairs(byKey[key] or {}) do if not i.dead and string.lower(i.path) == string.lower(path) then return true end end return false end
removeScript = function(path, key) for _, i in ipairs(byKey[key] or {}) do if string.lower(i.path) == string.lower(path) then i.dead = true end end end

-- A scripted type's flag (`.omwscripts`): the scripts an object of that record type gets.
local FLAG = { NPC_ = 'NPC', CREA = 'CREATURE', ACTI = 'ACTIVATOR', ARMO = 'ARMOR', BOOK = 'BOOK', CLOT = 'CLOTHING', CONT = 'CONTAINER', DOOR = 'DOOR',
  INGR = 'INGREDIENT', LIGH = 'LIGHT', MISC = 'MISC_ITEM', ALCH = 'POTION', WEAP = 'WEAPON', APPA = 'APPARATUS', LOCK = 'LOCKPICK', PROB = 'PROBE', REPA = 'REPAIR' }
local function has(flags, f) for _, x in ipairs(flags) do if x == f then return true end end return false end
local function attach(key)
  local d = S.objects[key]
  local flag = d and FLAG[d.tag or '']
  if not flag or d.attached then return end
  d.attached = true
  for _, r in ipairs(registered) do if has(r.flags, flag) then instantiate(r.path, key, 'local') end end
end
local function detach(key)
  for _, i in ipairs(byKey[key] or {}) do if not i.dead then call(i, i.handlers.onInactive) i.dead = true end end
  byKey[key] = nil
  ifaceLocal[key] = nil
end
activate = function(key, by)
  local o, a = handle(key), handle(by or 'player')
  for _, i in ipairs(byKey[key] or {}) do if not i.dead then call(i, i.handlers.onActivated, a) end end
  -- The global scripts' onActivate; OpenMW's activationhandlers.lua runs the standard action.
  local handled = false
  for _, i in ipairs(scripts) do if i.ctx == 'global' and not i.dead and i.handlers.onActivate then handled = true call(i, i.handlers.onActivate, o, a) end end
  if not handled then emit({ k = 'activate', key = key, by = by }) end
end

------------------------------------------------------------------- the frame, from the page
local function status()
  local g, p, l, bad = 0, 0, 0, json.array()
  for _, i in ipairs(scripts) do
    if not i.dead then
      if i.ctx == 'global' then g = g + 1 elseif i.ctx == 'player' then p = p + 1 else l = l + 1 end
      if (i.errors or 0) > 0 then bad[#bad + 1] = i.path end
    end
  end
  return { global = g, player = p, ['local'] = l, failing = bad, timers = #timers }
end
local function answer()
  local out = { effects = json.array(OUT), log = json.array(LOG), status = status() }
  OUT, LOG = {}, {}
  return json.encode(out)
end

-- `{scripts: [{path, flags}], sources: {path: text}, files: [vfs paths], weathers: [names]}`
function __wg_start(arg)
  local a = json.decode(arg)
  registered = a.scripts or {}
  for p, t in pairs(a.sources or {}) do sources[string.lower(p)] = t end
  S.files = a.files or {}
  S.weathers = a.weathers or {}
  if a.state then __wg_merge(a.state) end
  for _, r in ipairs(registered) do
    if has(r.flags, 'GLOBAL') then instantiate(r.path, nil, 'global') end
  end
  for _, r in ipairs(registered) do
    if has(r.flags, 'PLAYER') then instantiate(r.path, 'player', 'player') end
  end
  return answer()
end

-- What changed in the scene: `objects: {key: {id, tag, pos, rot, scale, enabled, cell} | false}`,
-- and the player, the clock, the weather, globals, actors, sounds and MWScript locals.
function __wg_merge(a)
  for k, d in pairs(a.objects or {}) do
    if d == false then
      if S.objects[k] then detach(k) S.objects[k] = nil end
    else
      local o = S.objects[k]
      if not o then o = {} S.objects[k] = o end
      for f, v in pairs(d) do o[f] = v end
      if not o.start then o.start = o.pos end
    end
  end
  if a.player then for f, v in pairs(a.player) do S.player[f] = v end end
  for _, f in ipairs({ 'hour', 'day', 'month', 'year', 'weather', 'musicPlaying', 'real' }) do if a[f] ~= nil then S[f] = a[f] end end
  for k, g in pairs(a.globals or {}) do S.globals[k] = g end
  for k, x in pairs(a.actors or {}) do S.actors[k] = x end
  if a.sounds then S.sounds = {} for _, s in ipairs(a.sounds) do S.sounds[s] = true end end
  for k, m in pairs(a.mwlocals or {}) do S.mwlocals[k] = m end
  for k, v in pairs(a.journal or {}) do S.journal[k] = v end
end

-- `{dt, ...what changed, activations: [{key, by}]}` -> `{effects, log, status}`
function __wg_step(arg)
  local a = json.decode(arg)
  __wg_merge(a)
  for k in pairs(a.objects or {}) do if S.objects[k] then attach(k) end end
  local dt = a.dt or 0
  S.sim = S.sim + dt
  S.dt = dt
  for _, e in ipairs(a.activations or {}) do activate(e.key, e.by) end
  if a.screen then S.screen = a.screen end
  for _, e in ipairs(a.ui or {}) do __wg_ui_event(e.id, e.path, e.event, e.x, e.y, e.text) end
  local g, l = globalQueue, localQueue
  globalQueue, localQueue = {}, {}
  local globals = {}
  for _, i in ipairs(scripts) do if i.ctx == 'global' and not i.dead then globals[#globals + 1] = i end end
  for _, e in ipairs(g) do deliver(globals, e[1], e[2]) end
  for _, e in ipairs(l) do deliver(byKey[e[1]] or {}, e[2], e[3]) end
  local due = {}
  for n = #timers, 1, -1 do if timers[n].at <= S.sim then due[#due + 1] = timers[n] table.remove(timers, n) end end
  for n = #due, 1, -1 do local t = due[n] call(t.inst, type(t.cb) == 'table' and (named[t.inst.path .. '|' .. t.cb.name] or t.cb.fn) or t.cb, t.arg) end
  for _, i in ipairs(scripts) do
    if not i.dead then
      call(i, i.handlers.onUpdate, dt)
      if i.ctx == 'player' then call(i, i.handlers.onFrame, dt) end
    end
  end
  local live = {}
  for _, i in ipairs(scripts) do if not i.dead then live[#live + 1] = i end end
  scripts = live
  return answer()
end

function __wg_status() return json.encode(status()) end
