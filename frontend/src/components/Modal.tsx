import { type ReactNode, useRef, useEffect } from 'react';
import './Modal.css';

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  footer?: ReactNode;
  size?: 'sm' | 'md' | 'lg' | 'xl' | 'xxl';
}

/**
 * General-purpose modal built on the native `<dialog>` element, with a titled
 * header, body, and optional footer. Opens/closes with the `open` prop; the
 * backdrop and Escape both call `onClose`. For yes/no prompts use ConfirmDialog.
 */
export function Modal({ open, onClose, title, children, size = 'md' , footer }: ModalProps) {
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (open && !el.open) {
      el.showModal();
    } else if (!open && el.open) {
      el.close();
    }
  }, [open]);

  const handleBackdropClick = (e: React.MouseEvent<HTMLDialogElement>) => {
    if (e.target === ref.current) onClose();
  };

  return (
    <dialog
      ref={ref}
      className={`modal modal--${size}`}
      onClick={handleBackdropClick}
      onCancel={(e) => { e.preventDefault(); onClose(); }}
    >
      <div className="modal__container">
        <div className="modal__header">
          <h2 className="modal__title">{title}</h2>
          <button className="modal__close" onClick={onClose} aria-label="Close">×</button>
        </div>
        <div className="modal__body">
          {children}
        </div>
        {footer && <div className="modal__footer">{footer}</div>}
      </div>
    </dialog>
  );
}
