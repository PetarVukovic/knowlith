import { Fragment, type ReactNode } from "react"
import { cn } from "@/lib/utils"

/**
 * Small Markdown renderer for canonical context bodies.
 *
 * Deliberately narrow: headings, paragraphs, lists, tables, bold, inline code.
 * Canonical objects are written by the compiler under a fixed grammar, so a
 * general Markdown dependency would buy nothing.
 */

function inline(text: string, keyPrefix: string): ReactNode[] {
  const nodes: ReactNode[] = []
  const pattern = /(\*\*[^*]+\*\*|`[^`]+`|\*[^*]+\*)/g
  let last = 0
  let match: RegExpExecArray | null
  let i = 0
  while ((match = pattern.exec(text)) !== null) {
    if (match.index > last) nodes.push(text.slice(last, match.index))
    const token = match[0]
    const key = `${keyPrefix}-${i++}`
    if (token.startsWith("**")) nodes.push(<strong key={key}>{token.slice(2, -2)}</strong>)
    else if (token.startsWith("`")) nodes.push(<code key={key}>{token.slice(1, -1)}</code>)
    else nodes.push(<em key={key}>{token.slice(1, -1)}</em>)
    last = match.index + token.length
  }
  if (last < text.length) nodes.push(text.slice(last))
  return nodes
}

const ORDERED = /^\s*\d+\.\s/
const BULLET = /^\s*[-*]\s/

interface ListItem {
  text: string
  children: string[]
}

function isTableRow(line: string) {
  return line.trim().startsWith("|") && line.trim().endsWith("|")
}

function cells(line: string) {
  return line
    .trim()
    .slice(1, -1)
    .split("|")
    .map((c) => c.trim())
}

export function Markdown({ source, className }: { source: string; className?: string }) {
  const lines = source.split("\n")
  const blocks: ReactNode[] = []
  let i = 0
  let key = 0

  while (i < lines.length) {
    const line = lines[i]
    if (!line.trim()) {
      i++
      continue
    }
    if (line.startsWith("### ")) {
      blocks.push(<h3 key={key++}>{inline(line.slice(4), `h${key}`)}</h3>)
      i++
    } else if (line.startsWith("## ")) {
      blocks.push(<h2 key={key++}>{inline(line.slice(3), `h${key}`)}</h2>)
      i++
    } else if (line.startsWith("# ")) {
      blocks.push(<h1 key={key++}>{inline(line.slice(2), `h${key}`)}</h1>)
      i++
    } else if (isTableRow(line)) {
      const head = cells(line)
      i++
      if (i < lines.length && /^\|[\s:|-]+\|$/.test(lines[i].trim())) i++
      const body: string[][] = []
      while (i < lines.length && isTableRow(lines[i])) {
        body.push(cells(lines[i]))
        i++
      }
      blocks.push(
        <div key={key++} className="-mx-1 overflow-x-auto px-1">
          <table>
            <thead>
              <tr>
                {head.map((c, ci) => (
                  <th key={ci}>{inline(c, `th${ci}`)}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {body.map((row, ri) => (
                <tr key={ri}>
                  {row.map((c, ci) => (
                    <td key={ci} className={/^[\d.,\s]+(EUR|%|kom)?$/.test(c) ? "num" : undefined}>
                      {inline(c, `td${ri}-${ci}`)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>,
      )
    } else if (ORDERED.test(line) || BULLET.test(line)) {
      const ordered = ORDERED.test(line)
      const items: ListItem[] = []
      while (i < lines.length && (ORDERED.test(lines[i]) || BULLET.test(lines[i]))) {
        const raw = lines[i]
        const indented = /^\s+/.test(raw)
        const text = raw.replace(/^\s*(?:\d+\.|[-*])\s/, "")
        if (indented && items.length > 0) items[items.length - 1].children.push(text)
        else items.push({ text, children: [] })
        i++
      }
      const List = ordered ? "ol" : "ul"
      blocks.push(
        <List key={key++}>
          {items.map((item, ii) => (
            <li key={ii}>
              {inline(item.text, `li${ii}`)}
              {item.children.length > 0 ? (
                <ul>
                  {item.children.map((child, ci) => (
                    <li key={ci}>{inline(child, `li${ii}-${ci}`)}</li>
                  ))}
                </ul>
              ) : null}
            </li>
          ))}
        </List>,
      )
    } else {
      const paragraph: string[] = []
      while (
        i < lines.length &&
        lines[i].trim() &&
        !lines[i].startsWith("#") &&
        !isTableRow(lines[i]) &&
        !BULLET.test(lines[i]) &&
        !ORDERED.test(lines[i])
      ) {
        paragraph.push(lines[i])
        i++
      }
      blocks.push(<p key={key++}>{inline(paragraph.join(" "), `p${key}`)}</p>)
    }
  }

  return (
    <div className={cn("doc-prose", className)}>
      {blocks.map((b, bi) => (
        <Fragment key={bi}>{b}</Fragment>
      ))}
    </div>
  )
}
