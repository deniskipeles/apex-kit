import React from 'react';
import { Loader2, ChevronDown } from 'lucide-react';

export const Button = React.forwardRef(
  (
    { children, variant = 'primary', size = 'md', isLoading, className = '', ...props }: any,
    ref: any
  ) => {
    const baseStyles =
      'inline-flex items-center justify-center rounded-md font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50';
    const variants: any = {
      primary: 'bg-primary text-primary-foreground shadow hover:bg-primary/90',
      secondary: 'bg-secondary text-secondary-foreground shadow-sm hover:bg-secondary/80',
      ghost: 'hover:bg-accent hover:text-accent-foreground',
      destructive: 'bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90',
      outline:
        'border border-input bg-transparent shadow-sm hover:bg-accent hover:text-accent-foreground',
      link: 'text-primary underline-offset-4 hover:underline',
    };
    const sizes: any = {
      sm: 'h-8 px-3 text-xs',
      md: 'h-9 px-4 py-2 text-sm',
      lg: 'h-10 px-8 text-base',
      icon: 'h-9 w-9',
    };

    return (
      <button
        ref={ref}
        className={`${baseStyles} ${variants[variant]} ${sizes[size]} ${className}`}
        disabled={isLoading || props.disabled}
        {...props}
      >
        {isLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
        {children}
      </button>
    );
  }
);
Button.displayName = 'Button';

export const Input = React.forwardRef(({ className, error, ...props }: any, ref) => {
  return (
    <div className="relative w-full">
      <input
        className={`flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 ${error ? 'border-destructive focus-visible:ring-destructive' : ''} ${className}`}
        ref={ref}
        {...props}
      />
      {error && (
        <span className="text-[10px] text-destructive absolute -bottom-4 left-0">{error}</span>
      )}
    </div>
  );
});
Input.displayName = 'Input';

export const Textarea = React.forwardRef(({ className, error, ...props }: any, ref) => {
  return (
    <div className="relative w-full">
      <textarea
        className={`flex min-h-[80px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 ${error ? 'border-destructive' : ''} ${className}`}
        ref={ref}
        {...props}
      />
      {error && (
        <span className="text-[10px] text-destructive absolute -bottom-4 left-0">{error}</span>
      )}
    </div>
  );
});
Textarea.displayName = 'Textarea';

export const Select = React.forwardRef(({ className, children, error, ...props }: any, ref) => {
  return (
    <div className="relative w-full">
      <div className="relative w-full">
        <select
          className={`flex h-9 w-full appearance-none rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 ${error ? 'border-destructive' : ''} ${className}`}
          ref={ref}
          {...props}
        >
          {children}
        </select>
        <ChevronDown className="absolute right-3 top-2.5 h-4 w-4 opacity-50 pointer-events-none" />
      </div>
      {error && (
        <span className="text-[10px] text-destructive absolute -bottom-4 left-0">{error}</span>
      )}
    </div>
  );
});
Select.displayName = 'Select';

export const Checkbox = React.forwardRef(({ className, ...props }: any, ref) => (
  <input
    type="checkbox"
    ref={ref}
    className={`h-4 w-4 rounded border-border bg-transparent text-primary focus:ring-1 focus:ring-primary ${className}`}
    {...props}
  />
));
Checkbox.displayName = 'Checkbox';

export const Switch = React.forwardRef(
  ({ checked, onCheckedChange, className, ...props }: any, ref) => (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onCheckedChange?.(!checked)}
      className={`peer inline-flex h-[24px] w-[44px] shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50 ${
        checked ? 'bg-primary' : 'bg-input'
      } ${className}`}
      ref={ref}
      {...props}
    >
      <span
        className={`pointer-events-none block h-5 w-5 rounded-full bg-background shadow-lg ring-0 transition-transform ${
          checked ? 'translate-x-5' : 'translate-x-0'
        }`}
      />
    </button>
  )
);
Switch.displayName = 'Switch';

export const Label = ({ className, children, required, ...props }: any) => (
  <label
    className={`text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70 ${className}`}
    {...props}
  >
    {children}
    {required && <span className="text-destructive ml-1">*</span>}
  </label>
);

export const Card = ({ className, children }: any) => (
  <div
    className={`rounded-xl border border-border bg-card text-card-foreground shadow ${className}`}
  >
    {children}
  </div>
);
export const CardHeader = ({ className, children }: any) => (
  <div className={`flex flex-col space-y-1.5 p-6 ${className}`}>{children}</div>
);
export const CardTitle = ({ className, children }: any) => (
  <h3 className={`font-semibold leading-none tracking-tight ${className}`}>{children}</h3>
);
export const CardContent = ({ className, children }: any) => (
  <div className={`p-6 pt-0 ${className}`}>{children}</div>
);

export const Badge = ({ variant = 'default', className, children }: any) => {
  const variants: any = {
    default: 'border-transparent bg-primary text-primary-foreground shadow hover:bg-primary/80',
    secondary: 'border-transparent bg-secondary text-secondary-foreground hover:bg-secondary/80',
    destructive:
      'border-transparent bg-destructive text-destructive-foreground shadow hover:bg-destructive/80',
    outline: 'text-foreground',
    success: 'border-transparent bg-emerald-500/15 text-emerald-500 hover:bg-emerald-500/25',
    warning: 'border-transparent bg-amber-500/15 text-amber-500 hover:bg-amber-500/25',
  };
  return (
    <div
      className={`inline-flex items-center rounded-md border px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 ${variants[variant]} ${className}`}
    >
      {children}
    </div>
  );
};
export const Skeleton = ({ className, ...props }: any) => (
  <div className={`animate-pulse rounded-md bg-muted ${className}`} {...props} />
);
export const Separator = ({ className, orientation = 'horizontal' }: any) => (
  <div
    className={`shrink-0 bg-border ${
      orientation === 'horizontal' ? 'h-[1px] w-full' : 'h-full w-[1px]'
    } ${className}`}
  />
);
