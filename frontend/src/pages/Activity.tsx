import { useEffect, useRef, useState } from 'react';
import { useMiner } from '../lib/state';
import { plainText, useT } from '../lib/i18n';
import { Button, Empty, Search } from '../components/ui';
export default function Activity() {
  const { data } = useMiner();
  const t = useT();
  const [search, setSearch] = useState('');
  const [following, setFollowing] = useState(true);
  const ref = useRef<HTMLDivElement>(null);
  const lines =
    data?.console.filter((line) => line.toLocaleLowerCase().includes(search.toLocaleLowerCase())) ??
    [];
  useEffect(() => {
    if (following && ref.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [data?.console, search, following]);
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-[22px] font-semibold">{t('activity')}</h1>
        <p className="mt-1 text-muted">{t('activity_description')}</p>
      </div>
      <div className="flex gap-3">
        <div className="flex-1">
          <Search value={search} onChange={setSearch} label={t('search_activity')} />
        </div>
        {!following && <Button onClick={() => setFollowing(true)}>{t('follow_activity')}</Button>}
      </div>
      <div
        ref={ref}
        tabIndex={0}
        aria-label={t('activity')}
        className="panel max-h-[calc(100dvh-260px)] min-h-72 overflow-y-auto px-4"
        onScroll={(event) => {
          const element = event.currentTarget;
          setFollowing(element.scrollHeight - element.scrollTop - element.clientHeight < 40);
        }}
      >
        {lines.map((line, index) => (
          <p
            className="break-words border-b border-divider py-3 text-[13px] leading-relaxed text-soft last:border-0"
            key={index}
          >
            {plainText(line)}
          </p>
        ))}
        {!lines.length && <Empty title={t('no_activity')} />}
      </div>
    </div>
  );
}
