import { Link } from "react-router-dom";
import { Button } from "@/components/ui/button";
import { ArrowLeft } from "lucide-react";

export function NotFoundPage() {
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-5 px-4 text-center">
      <p className="text-6xl font-bold tracking-tighter text-muted-foreground/40">
        404
      </p>
      <div>
        <h1 className="text-xl font-semibold">Page not found</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          The page you're looking for doesn't exist.
        </p>
      </div>
      <Button variant="outline" size="sm" render={<Link to="/" />}>
        <ArrowLeft className="h-3.5 w-3.5 mr-1.5" />
        Back to home
      </Button>
    </div>
  );
}
