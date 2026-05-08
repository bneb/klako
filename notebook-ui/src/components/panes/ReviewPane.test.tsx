import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ReviewPane } from './ReviewPane';
import { useStore } from '../../store/useStore';

describe('ReviewPane', () => {
  beforeEach(() => {
    useStore.setState({
      reviewVisible: true,
      reviewFilePath: 'docs/design.md',
      reviewContent: '# Main Header\n\nSome text.'
    });
  });

  it('renders content when visible', () => {
    render(<ReviewPane />);
    expect(screen.getByText('Review: docs/design.md')).toBeInTheDocument();
    expect(screen.getByText('Main Header')).toBeInTheDocument();
    expect(screen.getByText('Some text.')).toBeInTheDocument();
  });

  it('returns null when not visible', () => {
    useStore.setState({ reviewVisible: false });
    const { container } = render(<ReviewPane />);
    expect(container.firstChild).toBeNull();
  });
});
