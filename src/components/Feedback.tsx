import { AlertCircleIcon, CheckIcon } from "./Icons";
import "./Feedback.css";

export function Feedback({
  error,
  notice,
}: {
  error: { message: string } | null;
  notice: string | null;
}) {
  if (error) {
    return (
      <div className="pd-feedback pd-feedback-error">
        <AlertCircleIcon />
        <span>{error.message}</span>
      </div>
    );
  }
  if (notice) {
    return (
      <div className="pd-feedback pd-feedback-notice">
        <CheckIcon />
        <span>{notice}</span>
      </div>
    );
  }
  return null;
}
