-- How WoW: Forever answers each API ForeverBuddy.lua calls, by phase (H2).
--
-- The client isn't the retail one we first assumed. The character isn't up
-- at PLAYER_LOGIN, and it's torn down before PLAYER_LOGOUT. Every escape so
-- far (H0) was the fake being politer than that. So every API the addon
-- calls has a row here, and run.lua fails if one doesn't: a new call can't
-- land without saying what Forever returns while loading and during the
-- teardown.
--
-- Phases:
--   loading  from PLAYER_LOGIN until the bags load (a BAG_UPDATE_DELAYED a
--            moment after entering the world)
--   live     the character is up
--   torn     the logout teardown, from before PLAYER_LEAVING_WORLD on
--
-- Per phase (loading, torn), what the API returns instead of the live
-- answer; a missing key means "as live":
--   0      the number 0 (the first return; any others are nil)
--   "nil"  nothing
--   "{}"   an empty table
--
-- `from`: what we know it from.
--   "recording"  a real record-mode session shows it (tools/addon-test/README.md)
--   "report"     what Julia's client did, by ticket (`seen`)
--   "assumed"    nobody has seen it either way yet
-- `probe`: the record-mode probe that shows the API's answer, so a real
-- recording can confirm or contradict the row (run.lua's replay).

local NOT_UP = { loading = 0, torn = 0 }
local NONE = { loading = "nil", torn = "nil" }

local function row(base, fields)
    local r = {}
    for k, v in pairs(base or {}) do
        r[k] = v
    end
    for k, v in pairs(fields) do
        r[k] = v
    end
    return r
end

return {
    -- The character's state: what the teardown empties (BUG-SATCHELS,
    -- AUDIT-F) and what isn't there yet at login (AUDIT-F, BUG-ADV).
    ["C_Container.GetContainerNumSlots"] = row(NOT_UP, { from = "report", seen = "BUG-SATCHELS, BUG-ADV", probe = "b" }),
    ["C_Container.GetContainerItemID"] = row(NONE, { from = "report", seen = "BUG-SATCHELS" }),
    ["C_Container.GetContainerItemInfo"] = row(NONE, { from = "report", seen = "BUG-SATCHELS" }),
    ["C_Container.GetContainerNumFreeSlots"] = row(NOT_UP, { from = "assumed" }),
    ["C_Container.GetBagName"] = row(NONE, { from = "assumed" }),
    ["C_Item.GetItemCount"] = row(NOT_UP, { from = "assumed" }),
    GetMoney = row(NOT_UP, { from = "report", seen = "AUDIT-F", probe = "m" }),
    UnitXP = row(NOT_UP, { from = "report", seen = "AUDIT-F", probe = "x" }),
    UnitLevel = row(NOT_UP, { from = "report", seen = "AUDIT-F", probe = "l" }),
    UnitXPMax = { from = "assumed", probe = "xm" },
    GetXPExhaustion = { from = "assumed" },
    GetRestState = { from = "assumed" },
    GetInventoryItemID = row(NONE, { from = "report", seen = "AUDIT-F", probe = "w" }),
    GetInventoryItemLink = row(NONE, { from = "report", seen = "AUDIT-F" }),
    GetInventoryItemDurability = row(NONE, { from = "assumed" }),
    GetAverageItemLevel = row(NOT_UP, { from = "assumed" }),
    ["C_QuestLog.GetAllCompletedQuestIDs"] = { loading = "{}", torn = "{}", from = "report", seen = "AUDIT-F", probe = "q" },
    ["C_QuestLog.GetNumQuestLogEntries"] = row(NOT_UP, { from = "assumed" }),
    ["C_QuestLog.GetInfo"] = row(NONE, { from = "assumed" }),
    GetProfessions = row(NONE, { from = "report", seen = "AUDIT-F" }),
    GetProfessionInfo = row(NONE, { from = "report", seen = "AUDIT-F" }),
    GetNumSavedInstances = row(NOT_UP, { from = "report", seen = "AUDIT-F", probe = "sv" }),
    GetSavedInstanceInfo = row(NONE, { from = "report", seen = "AUDIT-F" }),
    ["C_Bank.FetchPurchasedBankTabIDs"] = row(NONE, { from = "assumed" }),
    GetRepairAllCost = row(NOT_UP, { from = "assumed" }),

    -- Who and where: the same in every phase as far as anyone has seen. The
    -- addon re-reads identity on the first live look anyway (S.startEarly).
    UnitName = { from = "report", seen = "probe run 1" },
    UnitGUID = { from = "report", seen = "probe run 1" },
    UnitClass = { from = "assumed" },
    UnitRace = { from = "assumed" },
    UnitSex = { from = "assumed" },
    UnitFactionGroup = { from = "assumed" },
    UnitIsPlayer = { from = "assumed" },
    GetRealmName = { from = "report", seen = "probe run 1" },
    GetGuildInfo = { from = "assumed" },
    GetRealZoneText = { from = "assumed" },
    GetSubZoneText = { from = "assumed" },
    IsInInstance = { from = "assumed" },
    GetInstanceInfo = { from = "assumed" },
    GetMaxPlayerLevel = { from = "assumed" },
    ["C_Map.GetBestMapForUnit"] = { from = "assumed" },
    ["C_Map.GetPlayerMapPosition"] = { from = "assumed" },

    -- The client itself: clocks, input, requests. No character state.
    GetServerTime = { from = "assumed" },
    GetTime = { from = "assumed" },
    GetBuildInfo = { from = "report", seen = "probe run 1" },
    GetCursorPosition = { from = "assumed" },
    IsShiftKeyDown = { from = "assumed" },
    InCombatLockdown = { from = "assumed" },
    ["C_Timer.After"] = { from = "assumed" },
    ["Settings.NotifyUpdate"] = { from = "assumed" },
    RequestTimePlayed = { from = "assumed" },
    ReloadUI = { from = "assumed" },
    RequestRaidInfo = { from = "assumed" },
    ["C_Map.SetUserWaypoint"] = { from = "assumed" },
    ["C_Map.ClearUserWaypoint"] = { from = "assumed" },
    ["UiMapPoint.CreateFromCoordinates"] = { from = "assumed" },
    ["Minimap.GetWidth"] = { from = "assumed" },
    date = { from = "assumed" },

    -- Item data: the client's cache, not the character.
    ["C_Item.GetItemInfo"] = { from = "assumed" },
    ["C_Item.GetDetailedItemLevelInfo"] = { from = "assumed" },
    ["C_Item.GetItemNameByID"] = { from = "assumed" },
    ["C_Item.GetItemSpell"] = { from = "assumed" },
    ["C_Item.RequestLoadItemDataByID"] = { from = "assumed" },
    ["C_TooltipInfo.GetItemByID"] = { from = "assumed" },

    -- Only answered while a window is open (the fake models that); the
    -- teardown closes them first.
    GetInboxNumItems = { from = "assumed" },
    GetInboxHeaderInfo = { from = "assumed" },
    GetInboxItem = { from = "assumed" },
    GetInboxItemLink = { from = "assumed" },
    GetMerchantNumItems = { from = "assumed" },
    GetMerchantItemID = { from = "assumed" },
    ["C_AuctionHouse.GetBrowseResults"] = { from = "assumed" },
    ["C_AuctionHouse.GetItemSearchResultInfo"] = { from = "assumed" },
    ["C_AuctionHouse.GetNumItemSearchResults"] = { from = "assumed" },
    ["C_QuestLog.GetTitleForQuestID"] = { from = "assumed" },
    ["C_QuestLog.IsComplete"] = { from = "assumed" },
    ["C_QuestLog.ReadyForTurnIn"] = { from = "assumed" },
    ["C_TradeSkillUI.GetAllRecipeIDs"] = { from = "assumed" },
    ["C_TradeSkillUI.GetBaseProfessionInfo"] = { from = "assumed" },
    ["C_TradeSkillUI.GetRecipeInfo"] = { from = "assumed" },
    ["C_TradeSkillUI.GetRecipeSchematic"] = { from = "assumed" },
    ["C_TradeSkillUI.IsNPCCrafting"] = { from = "assumed" },
    ["C_TradeSkillUI.IsTradeSkillGuild"] = { from = "assumed" },
    ["C_TradeSkillUI.IsTradeSkillLinked"] = { from = "assumed" },
}
