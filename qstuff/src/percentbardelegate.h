#ifndef PERCENTBARDELEGATE_H
#define PERCENTBARDELEGATE_H

#include <QStyledItemDelegate>

class PercentBarDelegate : public QStyledItemDelegate
{
public:
	PercentBarDelegate(int max=100, QObject* parent = nullptr);

	void paint(QPainter * painter, const QStyleOptionViewItem & option, const QModelIndex & index) const;

private:
	int m_maxValue;
};

#endif // PERCENTBARDELEGATE_H
