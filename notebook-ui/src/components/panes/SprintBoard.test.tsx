import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { SprintBoard } from './SprintBoard';
import { useStore } from '../../store/useStore';

describe('SprintBoard', () => {
  beforeEach(() => {
    useStore.setState({
      swarmTasks: [
        { description: 'Design Architecture', status: 'Completed' },
        { description: 'Setup Database', status: 'Running' },
        { description: 'Write Tests', status: 'Pending' }
      ]
    });
  });

  it('renders tasks in their respective columns', () => {
    render(<SprintBoard />);

    // Check Completed Column
    expect(screen.getByText('Design Architecture')).toBeInTheDocument();
    
    // Check Running Column
    expect(screen.getByText('Setup Database')).toBeInTheDocument();
    
    // Check Pending Column
    expect(screen.getByText('Write Tests')).toBeInTheDocument();
  });
});
