import { cn } from "@/lib/utils"

type Op = { type: "same" | "add" | "del"; text: string }

/** Longest common subsequence over lines — enough for canonical Markdown. */
function diffLines(before: string[], after: string[]): Op[] {
  const n = before.length
  const m = after.length
  const table: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0))
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      table[i][j] = before[i] === after[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1])
    }
  }
  const ops: Op[] = []
  let i = 0
  let j = 0
  while (i < n && j < m) {
    if (before[i] === after[j]) {
      ops.push({ type: "same", text: before[i] })
      i++
      j++
    } else if (table[i + 1][j] >= table[i][j + 1]) {
      ops.push({ type: "del", text: before[i] })
      i++
    } else {
      ops.push({ type: "add", text: after[j] })
      j++
    }
  }
  while (i < n) ops.push({ type: "del", text: before[i++] })
  while (j < m) ops.push({ type: "add", text: after[j++] })
  return ops
}

export function DiffView({
  before,
  after,
  className,
}: {
  before: string | null
  after: string
  className?: string
}) {
  const ops = diffLines((before ?? "").split("\n"), after.split("\n")).filter(
    (op, index, all) => !(op.type === "same" && op.text === "" && all[index - 1]?.text === ""),
  )

  return (
    <div className={cn("overflow-x-auto rounded-md border border-line bg-surface", className)}>
      <table className="w-full border-collapse font-mono text-[12px] leading-[1.7]">
        <tbody>
          {ops.map((op, index) => (
            <tr
              key={index}
              className={cn(
                op.type === "add" && "bg-add-soft",
                op.type === "del" && "bg-del-soft",
              )}
            >
              <td
                className={cn(
                  "w-6 select-none border-r border-line px-1.5 text-center align-top",
                  op.type === "add" && "text-add",
                  op.type === "del" && "text-del",
                  op.type === "same" && "text-faint",
                )}
              >
                {op.type === "add" ? "+" : op.type === "del" ? "−" : ""}
              </td>
              <td
                className={cn(
                  "whitespace-pre-wrap px-2.5 align-top",
                  op.type === "del" && "text-del line-through decoration-del/40",
                  op.type === "add" && "text-ink",
                  op.type === "same" && "text-muted",
                )}
              >
                {op.text || " "}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
