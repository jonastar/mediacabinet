import { useState } from 'react'
import { api, type Playlist, type SeriesDetail } from '../../api/client'
import { useApi } from '../../api/useApi'
import { Segmented } from '../../components/Segmented'
import { Failed, Loading } from '../../components/Status'
import { episodeLabel } from '../../lib/format'
import styles from './PlaylistEditor.module.css'

type Kind = 'smart' | 'manual'

const orderingId = (view: string) => Number(view.replace('ordering:', ''))

/**
 * Creating or changing a playlist of one series: an ordering without episodes
 * tagged, say, filler (it keeps up as episodes are added), or episodes picked
 * by hand.
 */
export function PlaylistEditor({
  series,
  playlist,
  canShare,
  onSaved,
  onCancel,
}: {
  series: SeriesDetail
  playlist?: Playlist
  canShare: boolean
  onSaved: (playlist: Playlist) => void
  onCancel: () => void
}) {
  const orderings = series.views.filter((v) => v.kind !== 'playlist')
  const defaultOrdering = orderings.find((v) => v.is_default) ?? orderings[0]
  const [name, setName] = useState(playlist?.name ?? '')
  const [kind, setKind] = useState<Kind>((playlist?.kind as Kind) ?? 'smart')
  const [base, setBase] = useState(
    playlist?.base_ordering_id
      ? `ordering:${playlist.base_ordering_id}`
      : (defaultOrdering?.id ?? ''),
  )
  const [tags, setTags] = useState((playlist?.exclude_tags ?? ['filler']).join(', '))
  const [picked, setPicked] = useState(new Set(playlist?.items ?? []))
  const [shared, setShared] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  // Episodes to pick from, in the series' default order.
  const episodes = useApi(kind === 'manual' ? `playlist-episodes:${series.id}` : null, () =>
    api.episodes(series.id, defaultOrdering?.id ?? null),
  )

  const toggle = (item: number) =>
    setPicked((current) => {
      const next = new Set(current)
      if (next.has(item)) {
        next.delete(item)
      } else {
        next.add(item)
      }
      return next
    })

  const save = async () => {
    setError(null)
    setSaving(true)
    const excludeTags = tags
      .split(',')
      .map((t) => t.trim())
      .filter(Boolean)
    // Picked episodes play in the series' order.
    const items = (episodes.data?.entries ?? [])
      .map((e) => e.item_id)
      .filter((item, i, all) => picked.has(item) && all.indexOf(item) === i)
    const changes =
      kind === 'smart'
        ? { name, base_ordering_id: orderingId(base), exclude_tags: excludeTags }
        : { name, items }
    try {
      onSaved(
        playlist
          ? await api.updatePlaylist(playlist.id, changes)
          : await api.createPlaylist({ ...changes, kind, shared }),
      )
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setSaving(false)
    }
  }

  return (
    <section className={styles.editor} aria-labelledby="playlist-editor">
      <h2 id="playlist-editor">{playlist ? `Change ${playlist.name}` : 'New playlist'}</h2>
      <label className={styles.field}>
        Name
        <input
          className={styles.input}
          value={name}
          placeholder="e.g. No filler"
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      {!playlist && (
        <Segmented
          label="Kind"
          variant="pills"
          value={kind}
          onChange={setKind}
          options={[
            { value: 'smart', label: 'Leave out tagged episodes' },
            { value: 'manual', label: 'Pick episodes' },
          ]}
        />
      )}
      {kind === 'smart' && (
        <>
          <label className={styles.field}>
            Follows
            <select className={styles.input} value={base} onChange={(e) => setBase(e.target.value)}>
              {orderings.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.name}
                </option>
              ))}
            </select>
          </label>
          <label className={styles.field}>
            Leave out episodes tagged
            <input
              className={styles.input}
              value={tags}
              placeholder="filler, recap"
              onChange={(e) => setTags(e.target.value)}
            />
            <span className={styles.hint}>
              Separate tags with commas. New episodes join by themselves.
            </span>
          </label>
        </>
      )}
      {kind === 'manual' && (
        <fieldset className={styles.picks}>
          <legend>Episodes ({picked.size} picked)</legend>
          {episodes.error && <Failed error={episodes.error} onRetry={episodes.reload} />}
          {!episodes.data && !episodes.error && <Loading label="Loading episodes…" />}
          {episodes.data?.entries.map((e, i) => (
            <label key={`${i}:${e.item_id}`} className={styles.pick}>
              <input
                type="checkbox"
                checked={picked.has(e.item_id)}
                onChange={() => toggle(e.item_id)}
              />
              <span className={styles.number}>{episodeLabel(e.number)}</span>
              {e.title}
            </label>
          ))}
        </fieldset>
      )}
      {canShare && !playlist && (
        <label className={styles.check}>
          <input type="checkbox" checked={shared} onChange={(e) => setShared(e.target.checked)} />
          Share with everyone
        </label>
      )}
      {error && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      <div className={styles.actions}>
        <button
          type="button"
          className={styles.primary}
          disabled={saving || !name.trim() || (kind === 'manual' && picked.size === 0)}
          onClick={save}
        >
          {playlist ? 'Save' : 'Create playlist'}
        </button>
        <button type="button" className={styles.button} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </section>
  )
}
