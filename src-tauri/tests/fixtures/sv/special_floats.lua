-- inf/nan spellings some C runtimes and addon serializers emit.
-- Not valid Lua for full_moon, so excluded from the differential test.
FloatsDB = {
	["inf"] = inf,
	["neg_inf"] = -inf,
	["div"] = 1/0,
	["neg_div"] = -1/0,
	["nan_div"] = 0/0,
	["msvc_inf"] = 1.#INF,
	["msvc_ind"] = -1.#IND,
	["msvc_qnan"] = 1.#QNAN,
	["ucrt_nan"] = -nan(ind),
	["glibc_nan"] = -nan,
}
