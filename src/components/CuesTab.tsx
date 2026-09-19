import { useStore } from '../store';
import type { Cue } from '../types';
import { Empty, Field, Panel } from './ui';

export function CuesTab() {
  const show = useStore((s) => s.show)!;
  const update = useStore((s) => s.update);
  const edit = (id: string, fn: (c: Cue) => void) =>
    update((sh) => {
      const c = sh.cues.find((x) => x.id === id);
      if (c) fn(c);
    });
  const add = () => {
    const next = Math.max(0, ...show.cues.map((c) => c.number ?? 0)) + 1;
    update((sh) => sh.cues.push({ id: `cue:${next}`, number: next, label: `Cue ${next}`, notes: '', steps: [{ kind: 'recall-scene', sceneId: sh.scenes[0]?.id }] }));
  };

  return (
    <Panel
      title={`Cue list (${show.cues.length})`}
      actions={
        <button type="button" className="btn small" onClick={add}>
          Add cue
        </button>
      }
    >
      {show.cues.length === 0 ? <Empty>No cues. A cue names the scene it recalls and carries the running-order notes; dLive and Avantis have a cue list of their own, the other desks recall scenes directly.</Empty> : null}
      {show.cues.length ? (
        <table className="table">
          <thead>
            <tr>
              <th>#</th>
              <th>Cue</th>
              <th>Recalls</th>
              <th>Notes</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {show.cues.map((c) => {
              const step = c.steps.find((s) => s.kind === 'recall-scene');
              return (
                <tr key={c.id}>
                  <td className="mono muted">
                    <input className="tiny-input" value={c.number ?? ''} onChange={(e) => edit(c.id, (x) => void (x.number = e.target.value ? Number(e.target.value) : undefined))} />
                  </td>
                  <td>
                    <input value={c.label} onChange={(e) => edit(c.id, (x) => void (x.label = e.target.value))} />
                  </td>
                  <td>
                    <Field label="">
                      <select
                        value={step?.sceneId ?? ''}
                        onChange={(e) =>
                          edit(c.id, (x) => {
                            const s = x.steps.find((st) => st.kind === 'recall-scene');
                            if (s) s.sceneId = e.target.value || undefined;
                            else x.steps.unshift({ kind: 'recall-scene', sceneId: e.target.value || undefined });
                          })
                        }
                      >
                        <option value="">—</option>
                        {show.scenes.map((s) => (
                          <option key={s.id} value={s.id}>
                            {s.bank ?? ''}
                            {s.number ?? ''} {s.label}
                          </option>
                        ))}
                      </select>
                    </Field>
                  </td>
                  <td>
                    <input value={c.notes} onChange={(e) => edit(c.id, (x) => void (x.notes = e.target.value))} />
                  </td>
                  <td>
                    <button type="button" className="btn tiny danger" onClick={() => update((sh) => void (sh.cues = sh.cues.filter((x) => x.id !== c.id)))}>
                      ×
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      ) : null}
    </Panel>
  );
}
