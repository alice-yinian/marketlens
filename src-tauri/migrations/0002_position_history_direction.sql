-- `position_history.pos_side` → `direction`
--
-- 原因：真机核对发现净持仓模式下 `posSide` 恒为 `net`，**真实方向在 `direction`
-- 字段**。原列名会把「持仓模式」和「多空方向」混为一谈——而方向是复盘统计里
-- 最基本的维度（多单胜率 vs 空单胜率），用错列名会让后续实现持续误解。
--
-- 该表在重命名前没有任何数据（历史仓位同步尚未实现），所以直接改名即可，
-- 不需要数据搬迁。

ALTER TABLE position_history RENAME COLUMN pos_side TO direction;
