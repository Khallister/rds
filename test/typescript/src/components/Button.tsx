// React component with alias imports
import React from 'react';
import type { ComponentProps } from '@/types';
import { validateInput } from '@/utils/validation';
import { createButtonTheme } from '~/themes';

interface ButtonProps extends ComponentProps {
  variant?: 'primary' | 'secondary' | 'danger';
  size?: 'small' | 'medium' | 'large';
  onClick?: () => void;
  disabled?: boolean;
}

export const Button: React.FC<ButtonProps> = ({
  children,
  variant = 'primary',
  size = 'medium',
  onClick,
  disabled = false,
  className = '',
  ...props
}) => {
  const theme = createButtonTheme(variant);
  
  const handleClick = () => {
    if (!disabled && onClick) {
          if (typeof children === 'string') {
        const sanitized = validateInput(children);
        console.log(`Button clicked: ${sanitized}`);
      }
      onClick();
    }
  };

  const baseClasses = `btn btn-${variant} btn-${size} ${theme.className}`;
  const finalClassName = `${baseClasses} ${className}`.trim();

  return (
    <button
      className={finalClassName}
      onClick={handleClick}
      disabled={disabled}
      style={theme.style}
      {...props}
    >
      {children}
    </button>
  );
};
