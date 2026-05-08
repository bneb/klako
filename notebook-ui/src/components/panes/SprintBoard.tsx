import { useStore } from '../../store/useStore';
import { motion, AnimatePresence } from 'framer-motion';

export const SprintBoard = () => {
  const { swarmTasks } = useStore();

  const pending = swarmTasks.filter(t => t.status === 'Pending');
  const running = swarmTasks.filter(t => t.status === 'Running' || t.status === 'Verifying');
  const done = swarmTasks.filter(t => t.status === 'Completed' || t.status === 'Failed');

  return (
    <div className="flex h-full w-full gap-4 p-4 bg-gray-50/50">
      <Column title="Open" tasks={pending} />
      <Column title="In Progress" tasks={running} />
      <Column title="Done" tasks={done} />
    </div>
  );
};

const Column = ({ title, tasks }: { title: string; tasks: any[] }) => (
  <div className="flex flex-col flex-1 bg-white border border-gray-200 rounded-xl shadow-sm overflow-hidden">
    <div className="bg-gray-100/50 border-b border-gray-200 px-4 py-3 font-semibold text-gray-700 text-sm">
      {title}
    </div>
    <div className="flex-1 p-3 overflow-y-auto flex flex-col gap-3">
      <AnimatePresence>
        {tasks.map((task, i) => (
          <motion.div
            key={`${task.description}-${i}`}
            layout
            initial={{ opacity: 0, y: 10, scale: 0.95 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, scale: 0.9 }}
            transition={{ type: 'spring', stiffness: 400, damping: 30 }}
            className={`p-3 rounded-lg border text-sm shadow-sm
              ${task.status === 'Running' || task.status === 'Verifying' 
                ? 'bg-emerald-50/30 border-emerald-200 shadow-emerald-100/50' 
                : 'bg-white border-gray-200'}
            `}
          >
            <div className="font-mono text-xs text-gray-400 mb-1">TASK-{String(i + 1).padStart(2, '0')}</div>
            <div className="text-gray-800 leading-snug">{task.description}</div>
            {(task.status === 'Verifying' || task.status === 'Failed') && (
              <div className="mt-2 text-xs font-semibold px-2 py-1 rounded bg-gray-100 text-gray-600 inline-block">
                {task.status}
              </div>
            )}
          </motion.div>
        ))}
      </AnimatePresence>
    </div>
  </div>
);
