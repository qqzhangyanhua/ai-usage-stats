import { EmptyState } from "../components/EmptyState";
import { LoadingOverlay } from "../components/LoadingOverlay";

export function ViewFallback() {
  return (
    <LoadingOverlay active className="panel partition">
      <EmptyState icon="overview" title="正在加载页面…" />
    </LoadingOverlay>
  );
}
