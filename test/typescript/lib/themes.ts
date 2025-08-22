// Theme utilities using tilde alias
interface ButtonTheme {
  className: string;
  style: {
    backgroundColor?: string;
    color?: string;
    border?: string;
  };
}

export function createButtonTheme(variant: string): ButtonTheme {
  switch (variant) {
    case 'primary':
      return {
        className: 'btn-theme-primary',
        style: {
          backgroundColor: '#007bff',
          color: 'white',
          border: '1px solid #007bff'
        }
      };
    case 'secondary':
      return {
        className: 'btn-theme-secondary',
        style: {
          backgroundColor: '#6c757d',
          color: 'white',
          border: '1px solid #6c757d'
        }
      };
    case 'danger':
      return {
        className: 'btn-theme-danger',
        style: {
          backgroundColor: '#dc3545',
          color: 'white',
          border: '1px solid #dc3545'
        }
      };
    default:
      return {
        className: 'btn-theme-default',
        style: {
          backgroundColor: '#f8f9fa',
          color: '#212529',
          border: '1px solid #dee2e6'
        }
      };
  }
}
