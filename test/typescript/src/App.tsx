// Main entry point with various import patterns
import { formatDate, calculateSum } from '@/utils';
import { Button } from '@/components/Button';
import type { User, ApiResponse } from '@/types';
import { debounce } from '~/helpers';
import React from 'react';

interface AppProps {
  user: User;
  onSubmit: (data: ApiResponse) => void;
}

const App: React.FC<AppProps> = ({ user, onSubmit }) => {
  const handleClick = debounce(() => {
    const result = calculateSum([1, 2, 3, 4, 5]);
    const date = formatDate(new Date());
    
    onSubmit({
      success: true,
      data: { result, date, user: user.name },
      message: 'Processing complete'
    });
  }, 300);

  return (
    <div>
      <h1>Hello {user.name}</h1>
      <p>Today is: {formatDate(new Date())}</p>
      <Button onClick={handleClick} variant="primary">
        Calculate Sum: {calculateSum([1, 2, 3])}
      </Button>
    </div>
  );
};

export default App;
