-- Writes an Auctionator.lua the way the game saves one, for the app's F5
-- tests (src-tauri/tests/fixtures/auctionator/Auctionator.lua):
--
--   luajit tools/addon-test/auctionator_fixture.lua > src-tauri/tests/fixtures/auctionator/Auctionator.lua
--
-- Each realm's prices are CBOR inside a Lua string, as Auctionator stores
-- them on modern clients (C_EncodingUtil.SerializeCBOR). The CBOR is built
-- by the small encoder below, apart from the app's decoder, and the string
-- is escaped with string.format("%q"), the escaping the game's serializer
-- uses, so the test reads bytes neither side of the app produced. Prices and
-- counts are picked so the CBOR holds a NUL, a newline, a carriage return, a
-- quote and a backslash: the bytes the escaping has to get right.
--
-- Not a substitute for a real Forever file: when one is available it
-- becomes a second fixture.

local function head(major, n)
    local m = major * 32
    if n < 24 then
        return string.char(m + n)
    elseif n < 256 then
        return string.char(m + 24, n)
    elseif n < 65536 then
        return string.char(m + 25, math.floor(n / 256), n % 256)
    else
        local b = {}
        for i = 4, 1, -1 do
            b[i] = n % 256
            n = math.floor(n / 256)
        end
        return string.char(m + 26, b[1], b[2], b[3], b[4])
    end
end

local function encode(v)
    if type(v) == "number" then
        return head(0, v)
    elseif type(v) == "string" then
        return head(3, #v) .. v
    end
    -- Maps, keys in a fixed order so the output never changes.
    local keys = {}
    for k in pairs(v) do
        keys[#keys + 1] = k
    end
    table.sort(keys)
    local out = { head(5, #keys) }
    for _, k in ipairs(keys) do
        out[#out + 1] = encode(k)
        out[#out + 1] = encode(v[k])
    end
    return table.concat(out)
end

local forever = {
    version = 2,
    -- Linen Cloth: two days; listed 10 and 13 (a newline and a CR in CBOR).
    ["2589"] = { m = 120, h = { ["2468"] = 140, ["2469"] = 125 }, l = { ["2469"] = 120 }, a = { ["2468"] = 10, ["2469"] = 13 } },
    -- Runecloth at 92 copper (a backslash), none listed at the end (a NUL).
    ["14047"] = { m = 92, h = { ["2469"] = 92 }, l = {}, a = { ["2469"] = 0 } },
    -- Black Lotus at 34 copper (a quote).
    ["13468"] = { m = 34, h = { ["2469"] = 34 }, l = {}, a = { ["2469"] = 1 } },
    -- Gear at an item level.
    ["g:19019:180"] = { m = 820000, h = { ["2465"] = 820000 }, l = {}, a = { ["2465"] = 1 } },
}

io.write("\nAUCTIONATOR_SAVEDVARS = {\n")
io.write('\t["TimeOfLastReplicateScan"] = 1791200000,\n')
io.write("}\n")
io.write("AUCTIONATOR_PRICE_DATABASE = {\n")
io.write('\t["__dbversion"] = 8,\n')
io.write('\t["Forever"] = ' .. string.format("%q", encode(forever)) .. ",\n")
io.write("}\n")
