import { ArrowRight } from 'lucide-react';
import { useProjectData } from './data';
import type { ProjectMemoriesResponse, ProjectOverviewResponse } from './service';
import { time } from './service';
import { Badge, Empty, Intro, QueryState } from './ui';
export function Home({ scope, project, navigate }: {scope: string; project: string; navigate: (page: string, id?: string) => void}) {
  const overview = useProjectData<ProjectOverviewResponse>(scope, project, 'overview');
  const memories = useProjectData<ProjectMemoriesResponse>(scope, project, 'memories');
  const recent = [...(memories.data?.items ?? [])].filter(m => m.status === 'active' && !m.is_tombstone).sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0,3);
  return <><Intro title="Pick up where you left off" subtitle="Your project knowledge and agent work, in one place."/>
    <QueryState query={overview}><section className="card accent continue"><span className="eyebrow">CONTINUE WORKING</span><div className="row"><div><strong>Continue work on {project}</strong><p className="muted">Review the latest checkpoint, changes and next steps.</p></div><button onClick={() => navigate('Sessions', 'briefing')}>Resume project <ArrowRight size={16}/></button></div><p className="muted">Last capture: {time(overview.data?.last_capture_at)}</p></section>
    <section className="card attention"><div className="row"><h2>Needs your attention</h2><Badge tone={overview.data?.pending_replacement_proposals ? 'warning' : 'positive'}>{overview.data?.pending_replacement_proposals ?? 0} to review</Badge></div><div className="row"><div><strong>{overview.data?.pending_replacement_proposals ? 'Knowledge updates are ready' : 'You’re up to date'}</strong><p className="muted">Compare proposed changes before adding them to project memory.</p></div><button onClick={() => navigate('Review')}>Review updates</button></div></section></QueryState>
    <section><div className="row section-heading"><h2>Recent knowledge</h2><button className="ghost" onClick={() => navigate('Memories')}>Browse memories <ArrowRight size={16}/></button></div><QueryState query={memories}><div className="three-columns">{recent.map(memory => <button className="card memory-card" key={memory.id} onClick={() => navigate('Memories', memory.id)}><strong>{memory.summary}</strong><p className="muted clamp">{memory.preview}</p></button>)}</div>{!recent.length && <Empty title="Your project knowledge will appear here">Memories become available after your first capture.</Empty>}</QueryState></section>
  </>;
}
