import { AlertDialog as AlertDialogPrimitive } from "bits-ui";
import Root from "./alert-dialog.svelte";
import Content from "./alert-dialog-content.svelte";
import Description from "./alert-dialog-description.svelte";
import Footer from "./alert-dialog-footer.svelte";
import Header from "./alert-dialog-header.svelte";
import Title from "./alert-dialog-title.svelte";

const Trigger = AlertDialogPrimitive.Trigger;
const Action = AlertDialogPrimitive.Action;
const Cancel = AlertDialogPrimitive.Cancel;

export {
	Root,
	Content,
	Description,
	Footer,
	Header,
	Title,
	Trigger,
	Action,
	Cancel,
	Root as AlertDialog,
	Content as AlertDialogContent,
	Description as AlertDialogDescription,
	Footer as AlertDialogFooter,
	Header as AlertDialogHeader,
	Title as AlertDialogTitle,
	Trigger as AlertDialogTrigger,
	Action as AlertDialogAction,
	Cancel as AlertDialogCancel,
};
