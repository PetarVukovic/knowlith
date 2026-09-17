import { useEffect, useState } from "react"
import { Link, useNavigate, useParams } from "react-router-dom"
import { AlertTriangle, ArrowLeft, ExternalLink, FileText } from "lucide-react"
import { KindIcon, kindMeta } from "@/components/Domain"
import { DocumentPreview } from "@/components/SourcePreview"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Panel, PanelHeader } from "@/components/ui/surface"
import { api } from "@/lib/api"
import type { Source, SourceDocumentIndex } from "@/lib/types"
import { formatRelative } from "@/lib/utils"
import { useApp } from "@/state/AppState"

/**
 * One folder's files and how each connects to company knowledge.
 *
 * The owner sees every document Knowlith has read from this source, which
 * objects quote it, and can open the stored snapshot even when the file
 * left the disk.
 */
export function SourceDetail() {
  const { sourceId } = useParams<{ sourceId: string }>()
  const navigate = useNavigate()
  const { sources, mode, live } = useApp()
  const [files, setFiles] = useState<SourceDocumentIndex[]>([])
  const [previewId, setPreviewId] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  const source: Source | undefined = sources.find((s) => s.id === sourceId)

  useEffect(() => {
    if (!sourceId || !live) {
      setLoading(false)
      return
    }
    let cancelled = false
    void (async () => {
      const rows = await api.getSourceDocumentIndex(sourceId)
      if (!cancelled) {
        setFiles(rows)
        setLoading(false)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [sourceId, live])

  if (!sourceId) {
    navigate("/sources", { replace: true })
    return null
  }

  const quoted = files.filter((f) => f.quotedBy.length > 0).length
  const missing = files.filter((f) => f.goneAt).length

  return (
    <div className="mx-auto w-full max-w-[880px] px-4 py-8">
      <Button variant="ghost" size="sm" className="mb-4 -ml-2" onClick={() => navigate("/sources")}>
        <ArrowLeft />
        Sources
      </Button>

      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <div>
          <h1 className="text-[20px] font-semibold tracking-[-0.015em] text-ink">{source?.name ?? sourceId}</h1>
          <p className="mt-1 max-w-[62ch] text-[13px] text-muted">
            Files Knowlith read from this folder and the knowledge that quotes each one.
          </p>
          {mode === "engineer" && source ? (
            <p className="mt-1 truncate font-mono text-[11.5px] text-faint">{source.path}</p>
          ) : null}
        </div>
      </div>

      <div className="mt-4 flex flex-wrap gap-2 text-[12.5px] text-muted">
        <span>{files.length} {files.length === 1 ? "file" : "files"} in the lake</span>
        <span>·</span>
        <span>{quoted} quoted in company knowledge</span>
        {missing > 0 ? (
          <>
            <span>·</span>
            <span className="text-conflict">{missing} no longer on disk</span>
          </>
        ) : null}
      </div>

      <div className="mt-6 grid gap-2">
        {loading ? (
          <p className="text-[13px] text-faint">Loading files…</p>
        ) : files.length === 0 ? (
          <Panel>
            <div className="p-4 text-[13px] text-muted">
              Nothing read from this folder yet. Read again from Sources when files are ready.
            </div>
          </Panel>
        ) : (
          files.map((file) => (
            <Panel key={file.id}>
              <div className="flex flex-wrap items-start gap-3 p-4">
                <div className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-md border border-line bg-surface-2">
                  <FileText className="size-4 text-faint" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-[14px] font-medium text-ink">{file.name}</span>
                    {file.goneAt ? (
                      <Badge tone="conflict">
                        <AlertTriangle className="size-3" />
                        Gone from disk
                      </Badge>
                    ) : null}
                    {file.quotedBy.length > 0 ? (
                      <Badge tone="confirmed">{file.quotedBy.length} quoted</Badge>
                    ) : (
                      <Badge tone="neutral">Not quoted yet</Badge>
                    )}
                  </div>
                  <div className="mt-0.5 text-[12.5px] text-muted">
                    Last read {formatRelative(file.modified)}
                    {mode === "engineer" ? ` · ${file.id}` : ""}
                  </div>
                  {mode === "engineer" ? (
                    <div className="mt-0.5 truncate font-mono text-[11.5px] text-faint">{file.path}</div>
                  ) : null}

                  {file.quotedBy.length > 0 ? (
                    <ul className="mt-3 grid gap-1">
                      {file.quotedBy.map((object) => (
                        <li key={object.id}>
                          <Link
                            to={
                              object.kind === "skill"
                                ? `/skills/${encodeURIComponent(object.id)}`
                                : `/workspace/${encodeURIComponent(object.id)}`
                            }
                            className="inline-flex items-center gap-1.5 rounded-md px-1 py-0.5 text-[12.5px] text-accent hover:bg-surface-2"
                          >
                            <KindIcon kind={object.kind} />
                            <span className="truncate">{object.title}</span>
                            <span className="text-faint">· {kindMeta(object.kind).label}</span>
                          </Link>
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="mt-2 text-[12.5px] text-faint">
                      No approved or waiting knowledge quotes this file yet.
                    </p>
                  )}
                </div>
                <Button size="sm" variant="ghost" onClick={() => setPreviewId(file.id)}>
                  <ExternalLink />
                  Open snapshot
                </Button>
              </div>
            </Panel>
          ))
        )}
      </div>

      {quoted > 0 ? (
        <Panel className="mt-8">
          <PanelHeader
            title="How this connects to the brain"
            description="Only files quoted by approved knowledge appear as document nodes in Company brain."
          />
          <div className="p-4 text-[13px] text-muted">
            Open{" "}
            <Link to="/brain" className="text-accent underline-offset-4 hover:underline">
              Company brain
            </Link>{" "}
            to see objects and the documents they quote. Double-click a document node to open its snapshot here.
          </div>
        </Panel>
      ) : null}

      <DocumentPreview documentId={previewId} open={previewId !== null} onOpenChange={(open) => !open && setPreviewId(null)} />
    </div>
  )
}
