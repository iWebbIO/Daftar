// PLACEHOLDER — being built. Replace this whole file.
import { Body, Empty, Head, Screen } from "../ui/kit";

export default function Order(props: { kind: "sale" | "purchase"; id?: number; partyId?: number }) {
  void props;
  return (
    <Screen>
      <Head title="فاکتور" />
      <Body>
        <Empty title="در حال ساخت" />
      </Body>
    </Screen>
  );
}