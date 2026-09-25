/**
 * 引导第 3 步：选择关注标的。
 *
 * 候选列表由 Rust 按 24h 成交额降序给出（低成交额合约的数据更容易被单笔大单扭曲），
 * `preselected` 为 true 的项默认勾选。上限 10 个是硬约束：标的数会直接乘进复盘请求量，
 * 所以超限时不仅禁用提交，还要把原因写出来，而不是只给一个灰按钮。
 */
import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { call } from "../../lib/ipc";
import { S } from "../../lib/strings";
import { errorPayloadOf } from "../live/errors";
import { formatPrice, formatUsd } from "../live/format";
import { WATCHLIST_LIMIT, validateWatchlistSelection } from "./validation";

export const WATCHLIST_CANDIDATES_KEY = ["watchlist_candidates"] as const;

export function StepWatchlist({ onDone }: { onDone: () => void }) {
  const candidates = useQuery({
    queryKey: WATCHLIST_CANDIDATES_KEY,
    queryFn: () => call("watchlist_candidates"),
  });
  const [selectedState, setSelectedState] = useState<string[] | null>(null);

  const initialSelection =
    candidates.data?.filter((candidate) => candidate.preselected).map((candidate) => candidate.inst_id) ??
    [];
  const selected = selectedState ?? initialSelection;
  const selectionError = validateWatchlistSelection(selected.length);

  const save = useMutation({
    mutationFn: (watchlist: string[]) => call("watchlist_set", { watchlist }),
    onSuccess: () => onDone(),
  });

  const toggle = (instId: string) => {
    setSelectedState(
      selected.includes(instId)
        ? selected.filter((id) => id !== instId)
        : [...selected, instId],
    );
  };

  const saveError = save.isError ? errorPayloadOf(save.error) : null;

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h2 className="text-lg font-semibold text-neutral-100">
          {S.onboarding.watchlist.title}
        </h2>
        <p className="mt-1 text-sm text-neutral-400">{S.onboarding.watchlist.hint}</p>
      </div>

      {candidates.isPending ? (
        <p className="text-sm text-neutral-400">{S.onboarding.watchlist.loading}</p>
      ) : candidates.isError ? (
        <div className="rounded-xl border border-red-900/60 bg-red-950/30 p-3 text-sm">
          <p className="font-mono break-all text-red-200/90">
            {errorPayloadOf(candidates.error).message}
          </p>
          <button
            type="button"
            onClick={() => void candidates.refetch()}
            className="mt-2 rounded-md bg-red-900/70 px-3 py-1.5 text-sm text-red-100 hover:bg-red-900"
          >
            {S.live.retry}
          </button>
        </div>
      ) : candidates.data.length === 0 ? (
        <p className="text-sm text-neutral-400">{S.onboarding.watchlist.empty}</p>
      ) : (
        <>
          <p className="text-xs text-neutral-500">
            {S.onboarding.watchlist.count(selected.length, WATCHLIST_LIMIT)}
          </p>
          <ul className="max-h-96 divide-y divide-neutral-800 overflow-y-auto rounded-xl border border-neutral-800">
            {candidates.data.map((candidate) => {
              const checked = selected.includes(candidate.inst_id);
              return (
                <li key={candidate.inst_id}>
                  <label className="flex cursor-pointer items-center gap-3 px-4 py-2.5 hover:bg-neutral-900">
                    <input type="checkbox" checked={checked} onChange={() => toggle(candidate.inst_id)} />
                    <span className="flex-1 font-mono text-sm text-neutral-100">
                      {candidate.inst_id}
                    </span>
                    <span className="text-right text-xs text-neutral-500">
                      <span className="font-mono tabular-nums">
                        {formatPrice(candidate.last) ?? S.account.na}
                      </span>
                      <span className="ml-3">
                        {S.onboarding.watchlist.volume}{" "}
                        <span className="font-mono tabular-nums">
                          {formatUsd(candidate.volume_24h_usd) ?? S.account.na}
                        </span>
                      </span>
                    </span>
                  </label>
                </li>
              );
            })}
          </ul>
        </>
      )}

      {selectionError === null ? null : (
        <p className="text-sm text-amber-300">{selectionError}</p>
      )}

      {saveError === null ? null : (
        <div className="rounded-xl border border-red-900/60 bg-red-950/30 p-3 text-sm">
          <p className="font-mono break-all text-red-200/90">{saveError.message}</p>
          <p className="mt-1 text-xs text-red-300/70">
            {S.onboarding.errorCode(saveError.code)}
          </p>
        </div>
      )}

      <button
        type="button"
        onClick={() => save.mutate(selected)}
        disabled={save.isPending || selectionError !== null}
        className="self-start rounded-md bg-neutral-100 px-4 py-2 text-sm font-medium text-neutral-900 hover:bg-white disabled:cursor-not-allowed disabled:opacity-50"
      >
        {save.isPending ? S.onboarding.watchlist.saving : S.onboarding.watchlist.submit}
      </button>
    </div>
  );
}
